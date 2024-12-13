use anyhow::{Context, Result};
use pulldown_cmark::{
    CowStr,
    Event::{self, *},
    HeadingLevel::*,
    MetadataBlockKind,
    Tag::*,
    TagEnd,
};
use std::{collections::HashMap, mem};

pub(crate) struct PreprocessedMarkdown<'a> {
    pub(crate) events: Vec<Event<'a>>,
    pub(crate) title_events: Option<Vec<Event<'a>>>,
    pub(crate) has_math: bool,
    pub(crate) metadata: Option<json::Value>,
}

pub(crate) fn preprocess<'a>(
    parser: impl Iterator<Item = Event<'a>>,
) -> Result<PreprocessedMarkdown<'a>> {
    let mut events = Vec::new();
    let mut title_events = None;
    let mut footnote_definitions = Vec::new();
    let mut has_math = false;
    let mut numbers = HashMap::new();
    let mut metadata = None;

    let mut state = State::default();
    for event in parser {
        if let InlineMath(..) | DisplayMath(..) = event {
            has_math = true;
        }

        if let FootnoteReference(ref label) | Start(FootnoteDefinition(ref label)) = event {
            let len = numbers.len();
            numbers.entry(label.clone()).or_insert(len);
        }

        state = match (mem::take(&mut state), &event) {
            (State::Default, Start(Heading { level: H1, .. })) if title_events.is_none() => {
                title_events = Some(Vec::new());
                events.push(event);
                State::Title
            }
            (State::Default, Start(FootnoteDefinition(label))) => {
                State::FootnoteDefinition(label.clone(), vec![event])
            }
            (State::Default, Start(MetadataBlock(MetadataBlockKind::PlusesStyle))) => {
                State::TomlMetadata(String::new())
            }
            (state @ State::Default, _) => {
                events.push(event);
                state
            }
            (State::Title, End(TagEnd::Heading(H1))) => {
                events.push(event);
                State::Default
            }
            (state @ State::Title, _) => {
                if let Some(title_events) = &mut title_events {
                    title_events.push(event.clone());
                }
                events.push(event);
                state
            }
            (State::FootnoteDefinition(label, mut events), End(TagEnd::FootnoteDefinition)) => {
                events.push(event);
                footnote_definitions.push((label, events));
                State::Default
            }
            (State::FootnoteDefinition(label, mut events), _) => {
                events.push(event);
                State::FootnoteDefinition(label, events)
            }
            (
                State::TomlMetadata(metadata_str),
                End(TagEnd::MetadataBlock(MetadataBlockKind::PlusesStyle)),
            ) => {
                let deserialized =
                    toml::from_str(&metadata_str).context("failed to parse frontmatter")?;
                metadata = Some(deserialized);
                State::Default
            }
            (State::TomlMetadata(mut metadata), Text(text)) => {
                metadata.push_str(text);
                State::TomlMetadata(metadata)
            }
            (State::TomlMetadata(metadata), _event) => State::TomlMetadata(metadata),
        };
    }

    footnote_definitions.sort_by_key(|(label, _)| numbers[label]);
    events.extend(
        footnote_definitions
            .into_iter()
            .flat_map(|(_, events)| events),
    );

    Ok(PreprocessedMarkdown {
        events,
        title_events,
        has_math,
        metadata,
    })
}

#[derive(Default, Clone)]
enum State<'a> {
    #[default]
    Default,
    Title,
    FootnoteDefinition(CowStr<'a>, Vec<Event<'a>>),
    TomlMetadata(String),
}
