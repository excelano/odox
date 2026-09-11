//! The zip container an `OpenDocument` document is: its entries, in the order
//! they were stored, and the rules about the first one.
//
// Author: David M. Anderson
// Built with AI assistance (Claude, Anthropic)

use std::io::{Cursor, Read, Write};

use zip::write::SimpleFileOptions;
use zip::{CompressionMethod, ZipArchive, ZipWriter};

use crate::Error;
use crate::xml::{self, Element, Ns};

/// One entry of a package.
pub struct Part {
    /// The entry's path inside the package, such as `content.xml` or
    /// `Pictures/100002010000...png`.
    pub name: String,
    /// The entry's bytes, decompressed.
    pub data: Vec<u8>,
    /// Whether the entry was stored rather than deflated.
    ///
    /// Kept so that writing a package it read does not recompress a picture
    /// that was already compressed when it went in, which costs time and grows
    /// the file.
    pub stored: bool,
    /// Whether the entry is a directory rather than a file.
    ///
    /// An office application writes a few of these — `Configurations2/` and
    /// `Thumbnails/` among them — and a package that comes back without them is
    /// a package that has been changed. They carry no bytes.
    pub directory: bool,
}

/// An `OpenDocument` package: a zip archive whose first entry declares the media
/// type of everything else in it.
///
/// Entries are kept in the order they were read, and every entry is kept
/// whether or not anything here understands it — a digital signature, a
/// thumbnail, an embedded object, a part from a future version of the format.
pub struct Package {
    parts: Vec<Part>,
    media_type: Option<String>,
}

/// The entry that carries the package's media type. It is first, stored rather
/// than deflated, and the one entry whose position in the archive the
/// specification fixes, so that a reader can identify the format from the first
/// bytes of the file without inflating anything.
const MIMETYPE: &str = "mimetype";

impl Package {
    /// Read a package from its bytes.
    ///
    /// # Errors
    ///
    /// The bytes are not a zip archive, or an entry cannot be decompressed.
    pub fn read(bytes: &[u8]) -> Result<Self, Error> {
        let mut archive = ZipArchive::new(Cursor::new(bytes))?;
        let mut parts = Vec::with_capacity(archive.len());
        for index in 0..archive.len() {
            let mut entry = archive.by_index(index)?;
            // A directory entry carries no bytes. It is kept rather than
            // skipped because an office application writes several, and a
            // package written back without them is not the package that was
            // read.
            let name = entry.name().to_owned();
            let stored = entry.compression() == CompressionMethod::Stored;
            if name.ends_with('/') {
                parts.push(Part {
                    name,
                    data: Vec::new(),
                    stored,
                    directory: true,
                });
                continue;
            }
            let mut data = Vec::new();
            entry
                .read_to_end(&mut data)
                .map_err(|e| Error::Package(e.to_string()))?;
            parts.push(Part {
                name,
                data,
                stored,
                directory: false,
            });
        }
        let mut package = Self {
            parts,
            media_type: None,
        };
        package.media_type = package.read_media_type();
        Ok(package)
    }

    /// The media type the package declares.
    ///
    /// `None` for a package that declares none, which is legal: the
    /// specification makes the `mimetype` entry optional, and a package written
    /// without one is identified by its file extension alone.
    pub fn media_type(&self) -> Option<&str> {
        self.media_type.as_deref()
    }

    /// Read the declared media type: the `mimetype` entry, or failing that the
    /// manifest's entry for the package as a whole, whose path is a single
    /// slash. Called once, when the package is read.
    fn read_media_type(&self) -> Option<String> {
        if let Some(part) = self.part(MIMETYPE)
            && let Ok(text) = std::str::from_utf8(&part.data)
        {
            let text = text.trim();
            if !text.is_empty() {
                return Some(text.to_owned());
            }
        }
        let part = self.part("META-INF/manifest.xml")?;
        let root = xml::parse(&part.data, "META-INF/manifest.xml").ok()?;
        root.elements()
            .find(|e| {
                e.is(&Ns::Manifest, "file-entry") && e.attr(&Ns::Manifest, "full-path") == Some("/")
            })
            .and_then(|e| e.attr(&Ns::Manifest, "media-type"))
            .filter(|t| !t.is_empty())
            .map(ToOwned::to_owned)
    }

    /// An entry by name.
    pub fn part(&self, name: &str) -> Option<&Part> {
        self.parts.iter().find(|p| p.name == name)
    }

    /// Every entry, in the order they are stored.
    pub fn parts(&self) -> impl Iterator<Item = &Part> {
        self.parts.iter()
    }

    /// Parse an entry as XML.
    ///
    /// # Errors
    ///
    /// The entry is absent, or is not well-formed XML.
    pub fn xml(&self, name: &'static str) -> Result<Element, Error> {
        let part = self.part(name).ok_or(Error::MissingPart(name))?;
        xml::parse(&part.data, name)
    }

    /// Parse an entry as XML, or give back an empty document if it is absent.
    ///
    /// `styles.xml`, `meta.xml` and `settings.xml` are each optional in a
    /// package that has nothing to put in them, and a document with no metadata
    /// is not a document that should fail to open.
    ///
    /// # Errors
    ///
    /// The entry is present and is not well-formed XML.
    pub fn optional_xml(&self, name: &'static str) -> Result<Option<Element>, Error> {
        match self.part(name) {
            Some(part) => xml::parse(&part.data, name).map(Some),
            None => Ok(None),
        }
    }

    /// Replace an entry's bytes, adding it at the end if it was not there.
    pub fn set_part(&mut self, name: &str, data: Vec<u8>) {
        if let Some(part) = self.parts.iter_mut().find(|p| p.name == name) {
            part.data = data;
            return;
        }
        self.parts.push(Part {
            name: name.to_owned(),
            data,
            stored: false,
            directory: false,
        });
    }

    /// Replace an entry with a serialized XML tree.
    pub fn set_xml(&mut self, name: &str, root: &Element) {
        self.set_part(name, xml::serialize(root));
    }

    /// Write the package back out.
    ///
    /// The `mimetype` entry goes first and uncompressed whatever order it was
    /// read in, because that is the one ordering rule the format has and a
    /// package that breaks it is identified by its extension alone.
    ///
    /// # Errors
    ///
    /// The underlying writer failed, which for a buffer in memory means the
    /// allocation did.
    pub fn write(&self) -> Result<Vec<u8>, Error> {
        let mut writer = ZipWriter::new(Cursor::new(Vec::new()));
        let stored = SimpleFileOptions::default().compression_method(CompressionMethod::Stored);
        let deflated = SimpleFileOptions::default().compression_method(CompressionMethod::Deflated);

        if let Some(part) = self.part(MIMETYPE) {
            writer.start_file(MIMETYPE, stored)?;
            writer
                .write_all(&part.data)
                .map_err(|e| Error::Package(e.to_string()))?;
        }
        for part in &self.parts {
            if part.name == MIMETYPE {
                continue;
            }
            if part.directory {
                writer.add_directory(part.name.trim_end_matches('/'), stored)?;
                continue;
            }
            let options = if part.stored { stored } else { deflated };
            writer.start_file(part.name.as_str(), options)?;
            writer
                .write_all(&part.data)
                .map_err(|e| Error::Package(e.to_string()))?;
        }
        Ok(writer.finish()?.into_inner())
    }

    /// Check the package declares one of the media types this reader opens.
    ///
    /// A format may have more than one: a Writer/Web document is a text document
    /// and declares a media type of its own.
    ///
    /// # Errors
    ///
    /// The package declares a media type that is none of them. A package that
    /// declares none is accepted: the caller asked for a format, and the file
    /// extension is the only other thing that ever said.
    pub fn expect_media_type(&self, wanted: &'static [&'static str]) -> Result<(), Error> {
        match self.media_type() {
            Some(found) if !wanted.contains(&found) => Err(Error::WrongFormat {
                found: found.to_owned(),
                wanted: wanted.first().copied().unwrap_or_default(),
            }),
            _ => Ok(()),
        }
    }
}
