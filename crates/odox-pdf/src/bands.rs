// Author: David M. Anderson
// Built with AI assistance (Claude, Anthropic)

use krilla::tagging::Tag;
use odox_core::doc::{PageFields, page_band, with_page_fields};
use odox_core::{Band, Document, Element, Ns, PageLayout, TextAlign};

use crate::Refusal;
use crate::layout::{Item, Layout};
use crate::model::{self, Block};

#[derive(Clone, Copy, PartialEq, Eq)]
pub(crate) enum Regions {
    Own,
    Sheet,
}

pub(crate) struct Bands<'a> {
    document: &'a Document,
    master: Option<&'a Element>,
    regions: Regions,
    left: f32,
    width: f32,
    header: Option<(Band, f32)>,
    footer: Option<(Band, f32)>,
}

impl<'a> Bands<'a> {
    pub(crate) fn new(
        layout: &mut Layout<'_>,
        document: &'a Document,
        master: Option<&'a Element>,
        page: &PageLayout,
        regions: Regions,
        fields: &PageFields<'_>,
    ) -> Result<Self, Refusal> {
        let margin = |length: Option<odox_core::Length>| length.map_or(0.0, |l| l.points());
        let left = margin(page.margin.left);
        let width = (page.width.points() - left - margin(page.margin.right)).max(72.0);
        let mut bands = Self {
            document,
            master,
            regions,
            left,
            width,
            header: None,
            footer: None,
        };
        for footer in [false, true] {
            let Some(band) = (if footer { page.footer } else { page.header }) else {
                continue;
            };
            let mut tallest: Option<f32> = None;
            for number in 1..=3 {
                let page_fields = PageFields {
                    page: number,
                    ..*fields
                };
                if let Some((_, height)) = bands.laid(layout, footer, &page_fields)? {
                    let height = height.max(band.min_height.points());
                    tallest = Some(tallest.map_or(height, |t| t.max(height)));
                }
            }
            if let Some(height) = tallest {
                if footer {
                    bands.footer = Some((band, height));
                } else {
                    bands.header = Some((band, height));
                }
            }
        }
        Ok(bands)
    }

    pub(crate) fn above(&self) -> f32 {
        self.header
            .map_or(0.0, |(band, height)| height + band.spacing.points())
    }

    pub(crate) fn below(&self) -> f32 {
        self.footer
            .map_or(0.0, |(band, height)| height + band.spacing.points())
    }

    pub(crate) fn draw(
        &self,
        layout: &mut Layout<'_>,
        fields: &PageFields<'_>,
        (top, bottom): (f32, f32),
    ) -> Result<Vec<(f32, Item)>, Refusal> {
        let mut placed = Vec::new();
        if self.header.is_some()
            && let Some((items, _)) = self.laid(layout, false, fields)?
        {
            placed.extend(items.into_iter().map(|item| (top - self.above(), item)));
        }
        if let Some((band, _)) = self.footer
            && let Some((items, _)) = self.laid(layout, true, fields)?
        {
            let at = bottom + band.spacing.points();
            placed.extend(items.into_iter().map(|item| (at, item)));
        }
        Ok(placed)
    }

    fn laid(
        &self,
        layout: &mut Layout<'_>,
        footer: bool,
        fields: &PageFields<'_>,
    ) -> Result<Option<(Vec<Item>, f32)>, Refusal> {
        let Some(band) = self
            .master
            .and_then(|master| page_band(master, footer, fields.page))
        else {
            return Ok(None);
        };
        let band = with_page_fields(band, fields, &self.document.styles);
        let regions: Vec<(Option<TextAlign>, &Element)> = match self.regions {
            Regions::Own => vec![(None, &band)],
            Regions::Sheet => {
                let found: Vec<_> = [
                    ("region-left", TextAlign::Start),
                    ("region-center", TextAlign::Center),
                    ("region-right", TextAlign::End),
                ]
                .into_iter()
                .filter_map(|(local, align)| {
                    band.child(&Ns::Style, local)
                        .map(|region| (Some(align), region))
                })
                .collect();
                if found.is_empty() {
                    vec![(Some(TextAlign::Center), &band)]
                } else {
                    found
                }
            }
        };
        let unread = layout.tree.group(None, Tag::Div);
        let mut items = Vec::new();
        let mut height: f32 = 0.0;
        for (align, content) in regions {
            let mut blocks = model::read(self.document, content, Vec::new(), None);
            if let Some(align) = align {
                for block in &mut blocks {
                    if let Block::Paragraph(paragraph) = block {
                        paragraph.align = align;
                    }
                }
            }
            let mut pieces = Vec::new();
            layout.blocks(&blocks, self.left, self.width, Some(unread), &mut pieces)?;
            let mut y = 0.0;
            for (index, piece) in pieces.into_iter().enumerate() {
                if index > 0 {
                    y += piece.before;
                }
                items.extend(
                    piece
                        .items
                        .into_iter()
                        .filter(|item| !matches!(item, Item::Link { .. } | Item::Empty { .. }))
                        .map(|item| item.shifted(0.0, y).decoration()),
                );
                y += piece.height + piece.after;
            }
            height = height.max(y);
        }
        Ok(Some((items, height)))
    }
}
