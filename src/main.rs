use anyhow::Result;
use clap::Parser;
use itertools::Itertools as _;
use std::io::Read as _;
use std::path::Path;
use std::str::FromStr;
use std::{fs, io};

#[derive(Debug, Parser)]
struct Args {
    /// Markdown file to render to HTML. Use `-` to read from stdin.
    #[arg(value_parser = path_is_file_or_std_stream)]
    file: String,
    /// An optional tera template to use for rendering.
    #[arg(short = 'T', long, value_parser = path_is_file)]
    template: Option<String>,
    /// Output file to write to. Defaults to <base_name(FILE)>.html.
    /// Use `-` to write to stdout instead.
    #[arg(short = 'O', long)]
    output: Option<String>,
    /// Explicitly set the title of the page.
    /// If not provided, the title will be extracted from the markdown file.
    #[arg(long)]
    title: Option<String>,
    /// Additional JSON metadata passed directly to the template.
    #[arg(long)]
    metadata: Option<Json>,
}

#[derive(Debug, Clone)]
struct Json(tera::Value);

impl FromStr for Json {
    type Err = <tera::Value as FromStr>::Err;

    fn from_str(s: &str) -> std::prelude::v1::Result<Self, Self::Err> {
        tera::Value::from_str(s).map(Json)
    }
}

fn main() -> Result<()> {
    let args = Args::parse();
    let markdown = read_input(&args.file)?;
    let title = args
        .title
        .or_else(|| extract_page_title(&markdown).map(to_plain_text));
    let body_html = markdown_to_html(&markdown);
    let math = has_math(&markdown);
    let html = args
        .template
        .map(|f| render_template(title.as_deref(), args.metadata, math, &body_html, &f))
        .unwrap_or(Ok(body_html))?;
    write_output(&output_file_name(&args.file, args.output), &html)?;
    Ok(())
}

fn markdown_to_html(markdown: &str) -> String {
    let parser = create_markdown_parser(markdown);
    let mut html = String::new();
    pulldown_cmark::html::push_html(&mut html, parser);
    html
}

fn render_template(
    title: Option<&str>,
    metadata: Option<Json>,
    math: bool,
    content: &str,
    template_file: &str,
) -> Result<String> {
    const TEMPLATE_NAME: &str = "template";
    let mut tera = tera::Tera::default();
    tera.add_raw_template(TEMPLATE_NAME, &fs::read_to_string(template_file)?)?;
    let mut context = tera::Context::new();
    context.insert("title", &title);
    context.insert("content", content);
    context.insert("metadata", &metadata.map(|m| m.0));
    context.insert("math", &math);
    Ok(tera.render(TEMPLATE_NAME, &context)?)
}

fn output_file_name(input_file: &str, output_file: Option<String>) -> String {
    output_file
        .or_else(|| {
            let base_name = Path::new(input_file).file_stem()?.to_string_lossy();
            Some(format!("{}.html", base_name))
        })
        .unwrap_or_else(|| "output.html".to_owned())
}

fn write_output(output_file: &str, content: &str) -> Result<()> {
    if output_file == "-" {
        println!("{}", content);
    } else {
        fs::write(output_file, content)?;
    }
    Ok(())
}

fn path_is_file(path: &str) -> Result<String, String> {
    if Path::new(path).is_file() {
        Ok(path.to_owned())
    } else {
        Err(format!("'{path}' does not exist or is not a file."))
    }
}

fn path_is_file_or_std_stream(path: &str) -> Result<String, String> {
    if path == "-" {
        Ok(path.to_owned())
    } else {
        path_is_file(path)
    }
}

fn read_input(path: &str) -> io::Result<String> {
    if path == "-" {
        let mut buffer = String::new();
        std::io::stdin().read_to_string(&mut buffer)?;
        Ok(buffer)
    } else {
        fs::read_to_string(path)
    }
}

fn extract_page_title(markdown: &str) -> Option<impl Iterator<Item = pulldown_cmark::Event>> {
    use pulldown_cmark::{Event, HeadingLevel::*, Tag, TagEnd};
    let mut parser = create_markdown_parser(markdown);

    loop {
        match parser.next()? {
            Event::Start(Tag::Heading { level: H1, .. }) => {
                return Some(parser.take_while(|e| !matches!(e, Event::End(TagEnd::Heading(H1)))))
            }
            Event::Start(Tag::MetadataBlock(_)) => {
                while !matches!(parser.next()?, Event::End(TagEnd::MetadataBlock(_))) {}
            }
            _ => return None,
        }
    }
}

fn has_math(markdown: &str) -> bool {
    use pulldown_cmark::Event::*;
    create_markdown_parser(markdown).any(|e| matches!(e, InlineMath(..) | DisplayMath(..)))
}

fn to_plain_text<'a>(events: impl Iterator<Item = pulldown_cmark::Event<'a>>) -> String {
    events
        .filter_map(|e| {
            if let pulldown_cmark::Event::Text(t) = e {
                Some(t)
            } else {
                None
            }
        })
        .join("")
}

fn create_markdown_parser(markdown: &str) -> pulldown_cmark::Parser {
    use pulldown_cmark::{Options, Parser};
    let options = Options::ENABLE_STRIKETHROUGH
        | Options::ENABLE_FOOTNOTES
        | Options::ENABLE_TABLES
        | Options::ENABLE_TASKLISTS
        | Options::ENABLE_YAML_STYLE_METADATA_BLOCKS
        | Options::ENABLE_PLUSES_DELIMITED_METADATA_BLOCKS
        | Options::ENABLE_MATH;
    Parser::new_ext(markdown, options)
}
