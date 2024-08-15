use pulldown_cmark::{
    Event::{self, *},
    HeadingLevel::*,
    Tag::*,
    TagEnd,
};

pub(crate) struct PreprocessedMarkdown<'a> {
    pub(crate) events: Vec<Event<'a>>,
    pub(crate) title_events: Option<Vec<Event<'a>>>,
    pub(crate) has_math: bool,
}

pub(crate) fn preprocess<'a>(parser: impl Iterator<Item = Event<'a>>) -> PreprocessedMarkdown<'a> {
    let mut events = Vec::new();
    let mut title_events = None;
    let mut footnote_definitions = Vec::new();
    let mut has_math = false;

    let mut state = State::default();
    for event in parser {
        if let InlineMath(..) | DisplayMath(..) = event {
            has_math = true;
        }
        match (state, &event) {
            (State::Default, Start(Heading { level: H1, .. })) => {
                if title_events.is_none() {
                    title_events = Some(Vec::new());
                    state = State::Title;
                }
                events.push(event);
            }
            (State::Default, Start(FootnoteDefinition(_))) => {
                state = State::FootnoteDefinition;
                footnote_definitions.push(event);
            }
            (State::Default, _) => events.push(event),
            (State::Title, End(TagEnd::Heading(H1))) => {
                state = State::default();
                events.push(event);
            }
            (State::Title, _) => {
                if let Some(title_events) = &mut title_events {
                    title_events.push(event.clone());
                }
                events.push(event);
            }
            (State::FootnoteDefinition, End(TagEnd::FootnoteDefinition)) => {
                state = State::default();
                footnote_definitions.push(event);
            }
            (State::FootnoteDefinition, _) => footnote_definitions.push(event),
        }
    }

    events.extend(footnote_definitions);

    PreprocessedMarkdown {
        events,
        title_events,
        has_math,
    }
}

#[derive(Default, Clone, Copy)]
enum State {
    #[default]
    Default,
    Title,
    FootnoteDefinition,
}
