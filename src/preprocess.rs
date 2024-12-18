use crate::highlight::highlight_code;
use anyhow::{Context, Result};
use pulldown_cmark::{
    CowStr,
    Event::{self, *},
    HeadingLevel::*,
    MetadataBlockKind,
    Tag::*,
    TagEnd,
};
use std::collections::HashMap;
use std::mem;

pub(crate) struct PreprocessedMarkdown<'a> {
    pub(crate) events: Vec<Event<'a>>,
    pub(crate) title_events: Option<Vec<Event<'a>>>,
    pub(crate) has_math: bool,
    pub(crate) has_highlighted_code: bool,
    pub(crate) metadata: Option<json::Value>,
}

pub(crate) fn preprocess<'a>(
    parser: impl Iterator<Item = Event<'a>>,
    highlight_command: Option<String>,
) -> Result<PreprocessedMarkdown<'a>> {
    let mut events = Vec::new();
    let mut title_events = None;
    let mut footnote_definitions = Vec::new();
    let mut has_math = false;
    let mut has_highlighted_code = false;
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

        state = match (mem::take(&mut state), event) {
            (State::Default, e @ Start(Heading { level: H1, .. })) if title_events.is_none() => {
                title_events = Some(Vec::new());
                events.push(e);
                State::Title
            }
            (State::Default, ref e @ Start(FootnoteDefinition(ref label))) => {
                State::FootnoteDefinition(label.clone(), vec![e.clone()])
            }
            (State::Default, Start(MetadataBlock(MetadataBlockKind::PlusesStyle))) => {
                State::TomlMetadata(String::new())
            }
            (State::Default, Start(CodeBlock(pulldown_cmark::CodeBlockKind::Fenced(tag))))
                if !tag.is_empty() && highlight_command.is_some() =>
            {
                State::FencedCodeBlock {
                    code: String::new(),
                    tag,
                }
            }
            (state @ State::Default, e) => {
                events.push(e);
                state
            }
            (State::Title, e @ End(TagEnd::Heading(H1))) => {
                events.push(e);
                State::Default
            }
            (state @ State::Title, e) => {
                if let Some(title_events) = &mut title_events {
                    title_events.push(e.clone());
                }
                events.push(e);
                state
            }
            (State::FootnoteDefinition(label, mut events), e @ End(TagEnd::FootnoteDefinition)) => {
                events.push(e);
                footnote_definitions.push((label, events));
                State::Default
            }
            (State::FootnoteDefinition(label, mut events), event) => {
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
                metadata.push_str(&text);
                State::TomlMetadata(metadata)
            }
            (State::TomlMetadata(metadata), event) => {
                eprintln!("unexpected event while reading TOML metadata: {event:?}");
                State::TomlMetadata(metadata)
            }
            (State::FencedCodeBlock { mut code, tag }, Text(text)) => {
                code.push_str(&text);
                State::FencedCodeBlock { code, tag }
            }
            (State::FencedCodeBlock { code, tag }, End(TagEnd::CodeBlock)) => {
                has_highlighted_code = true;
                let command = highlight_command
                    .as_deref()
                    .unwrap_or_else(|| unreachable!());
                events.extend([
                    Event::Start(HtmlBlock),
                    Event::Html(highlight_code(command, tag, code)?.into()),
                    Event::End(TagEnd::HtmlBlock),
                ]);
                State::Default
            }
            (State::FencedCodeBlock { code, tag }, event) => {
                eprintln!("unexpected event while reading code block: {event:?}");
                State::FencedCodeBlock { code, tag }
            }
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
        has_highlighted_code,
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
    FencedCodeBlock {
        code: String,
        tag: CowStr<'a>,
    },
}
