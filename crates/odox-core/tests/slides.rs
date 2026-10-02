//! Moving, deleting and duplicating slides: what each leaves behind.
//
// Author: David M. Anderson
// Built with AI assistance (Claude, Anthropic)

use std::collections::HashSet;
use std::path::Path;

use odox_core::doc::Presentation;
use odox_core::{Element, Node, Ns, Refused};

fn deck(name: &str) -> Presentation {
    let bytes = std::fs::read(
        Path::new(env!("CARGO_MANIFEST_DIR"))
            .join("../../corpus/libreoffice")
            .join(name),
    )
    .expect("the corpus deck");
    Presentation::read(&bytes).expect("it reads")
}

fn names(deck: &Presentation) -> Vec<String> {
    deck.slides()
        .iter()
        .map(|slide| slide.name.unwrap_or_default().to_owned())
        .collect()
}

fn ids(element: &Element, into: &mut Vec<String>) {
    for attribute in &element.attrs {
        if attribute.name.local.as_ref() == "id"
            && (attribute.name.is(&Ns::Draw, "id")
                || attribute.name.prefix.as_deref() == Some("xml"))
        {
            into.push(attribute.value.clone());
        }
    }
    for child in element.elements() {
        ids(child, into);
    }
}

fn has_connector(element: &Element) -> bool {
    element
        .attrs
        .iter()
        .any(|a| a.name.local.as_ref() == "start-shape")
        || element.elements().any(has_connector)
}

fn ids_of(element: &Element, kind: &str, into: &mut Vec<String>) {
    for attribute in &element.attrs {
        let prefix = attribute.name.prefix.as_deref();
        if attribute.name.local.as_ref() == "id" && prefix == Some(kind) {
            into.push(attribute.value.clone());
        }
    }
    for child in element.elements() {
        ids_of(child, kind, into);
    }
}

#[test]
fn a_slide_moves_one_place_and_nothing_else_changes() {
    let mut deck = deck("deck.odp");
    let before = names(&deck);
    let position = deck.slides()[1].position;
    let moved = deck.move_slide(position, true).expect("later");
    assert_eq!(deck.slides()[2].position, moved);
    let mut expected = before.clone();
    expected.swap(1, 2);
    assert_eq!(names(&deck), expected);
    let back = deck.move_slide(moved, false).expect("earlier");
    assert_eq!(names(&deck), before);
    assert_eq!(back, position);
}

#[test]
fn the_first_slide_cannot_move_earlier_nor_the_last_later() {
    let mut deck = deck("deck.odp");
    let slides = deck.slides();
    let (first, last) = (slides[0].position, slides[slides.len() - 1].position);
    drop(slides);
    assert_eq!(deck.move_slide(first, false), Err(Refused::NotFound));
    assert_eq!(deck.move_slide(last, true), Err(Refused::NotFound));
}

#[test]
fn deleting_takes_the_slide_and_its_notes_and_the_last_one_stays() {
    let mut deck = deck("deck.odp");
    let before = names(&deck);
    let position = deck.slides()[1].position;
    deck.delete_slide(position).expect("deleted");
    let mut expected = before;
    expected.remove(1);
    assert_eq!(names(&deck), expected);

    while deck.slides().len() > 1 {
        let position = deck.slides()[0].position;
        deck.delete_slide(position).expect("deleted");
    }
    let only = deck.slides()[0].position;
    assert_eq!(deck.delete_slide(only), Err(Refused::LastOne));
    assert_eq!(deck.slides().len(), 1);
}

#[test]
fn a_duplicate_has_a_name_of_its_own_and_ids_of_its_own_and_its_connectors_follow() {
    let mut deck = deck("growing-liberty.odp");
    let (position, _) = {
        let slides = deck.slides();
        let found = slides
            .iter()
            .find(|slide| has_connector(slide.element))
            .expect("a slide with a connector");
        (found.position, found.name.map(ToOwned::to_owned))
    };
    let count = deck.slides().len();
    let copy = deck.duplicate_slide(position).expect("duplicated");
    assert_eq!(copy, position + 1);
    assert_eq!(deck.slides().len(), count + 1);

    let all = names(&deck);
    assert_eq!(
        all.iter().collect::<HashSet<_>>().len(),
        all.len(),
        "names are unique"
    );

    // Both attributes name an element, and LibreOffice writes the same value
    // in each, so each kind is counted by itself.
    for kind in ["xml", "draw"] {
        let mut every = Vec::new();
        ids_of(&deck.document.content, kind, &mut every);
        let unique: HashSet<_> = every.iter().collect();
        assert_eq!(unique.len(), every.len(), "no {kind} id is there twice");
    }

    // What the copy's connectors name is in the copy.
    let slides = deck.slides();
    let copied = slides
        .iter()
        .find(|s| s.position == copy)
        .expect("the copy");
    let mut inside = Vec::new();
    ids(copied.element, &mut inside);
    let inside: HashSet<_> = inside.into_iter().collect();
    let mut references = 0;
    fn walk(element: &Element, inside: &HashSet<String>, references: &mut usize) {
        for attribute in &element.attrs {
            if matches!(attribute.name.local.as_ref(), "start-shape" | "end-shape") {
                *references += 1;
                assert!(
                    inside.contains(&attribute.value),
                    "{} is the copy's",
                    attribute.value
                );
            }
        }
        for child in element.elements() {
            walk(child, inside, references);
        }
    }
    walk(copied.element, &inside, &mut references);
    assert!(references > 0, "the slide had connectors to follow");
}

#[test]
fn a_deck_with_slides_added_and_taken_away_saves_and_reads_back() {
    let mut deck = deck("deck.odp");
    let position = deck.slides()[0].position;
    deck.duplicate_slide(position).expect("duplicated");
    let position = deck.slides()[2].position;
    deck.delete_slide(position).expect("deleted");
    let position = deck.slides()[1].position;
    deck.move_slide(position, true).expect("moved");
    let written = deck
        .document
        .write_verified()
        .expect("it comes back the same");
    let again = Presentation::read(&written).expect("it reads again");
    assert_eq!(names(&again), names(&deck));
}

#[test]
fn a_deleted_slide_leaves_the_custom_shows_and_the_start_page() {
    let mut deck = deck("deck.odp");
    let slides = deck.slides();
    let (first, second) = (
        slides[0].name.expect("named").to_owned(),
        slides[1].name.expect("named").to_owned(),
    );
    let at = slides[0].position;
    drop(slides);
    let presentation = deck
        .document
        .content
        .child_mut(&Ns::Office, "body")
        .and_then(|b| b.child_mut(&Ns::Office, "presentation"))
        .expect("a presentation");
    let mut show = Element::new("presentation", "show", Ns::Presentation);
    show.set_attr(
        odox_core::Name::new("presentation", "name", Ns::Presentation),
        "mine",
    );
    show.set_attr(
        odox_core::Name::new("presentation", "pages", Ns::Presentation),
        format!("{first},{second}"),
    );
    let mut only = show.clone();
    only.set_attr(
        odox_core::Name::new("presentation", "pages", Ns::Presentation),
        first.clone(),
    );
    presentation.children.push(Node::Element(show));
    presentation.children.push(Node::Element(only));
    deck.delete_slide(at).expect("deleted");
    let shows: Vec<_> = deck
        .document
        .content
        .child(&Ns::Office, "body")
        .and_then(|b| b.child(&Ns::Office, "presentation"))
        .expect("a presentation")
        .elements()
        .filter(|e| e.is(&Ns::Presentation, "show"))
        .map(|e| {
            e.attr(&Ns::Presentation, "pages")
                .unwrap_or_default()
                .to_owned()
        })
        .collect();
    assert_eq!(
        shows,
        [second],
        "the show that listed it keeps the others and the one that held only it is gone"
    );
}
