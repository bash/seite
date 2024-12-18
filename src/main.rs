use anyhow::Result;
use clap::Parser;
use itertools::Itertools as _;
use preprocess::preprocess;
use std::io::Read as _;
use std::path::Path;
use std::str::FromStr;
use std::{fs, io};

mod highlight;
mod preprocess;

#[derive(Debug, Parser)]
struct Args {
    /// Markdown file to render to HTML. Use `-` to read from stdin.
    #[arg(value_parser = path_is_file_or_std_stream)]
    file: String,
    /// An optional tera template to use for rendering.
    /// Additional values are added to the tera context for inheritance.
    #[arg(short = 'T', long, value_parser = path_is_file)]
    template: Vec<String>,
    /// Output file to write to. Defaults to <base_name(FILE)>.html.
    /// Use `-` to write to stdout instead.
    #[arg(short = 'O', long)]
    output: Option<String>,
    /// Explicitly set the title of the page.
    /// If not provided, the title will be extracted from the markdown file.
    #[arg(long)]
    title: Option<String>,
    /// Additional JSON metadata that is merged together with the
    /// metadata from the frontmatter. Passed directly to the template via the
    /// `metadata` variable.
    #[arg(long)]
    metadata: Option<Json>,
    /// Optional command to used to highlight code blocks.
    ///
    /// Any {} placeholder is replaced with the language name.
    /// The command is split according to shell splitting rules of the UNIX shell.
    /// Code is passed via stdin and HTML is expected on stdout.
    ///
    /// Example:
    /// pygmentize -f html -O cssclass=syntax -l {}
    #[arg(long)]
    highlight_command: Option<String>,
}

#[derive(Debug, Clone)]
struct Json(json::Value);

impl FromStr for Json {
    type Err = <json::Value as FromStr>::Err;

    fn from_str(s: &str) -> std::prelude::v1::Result<Self, Self::Err> {
        json::Value::from_str(s).map(Json)
    }
}

fn main() -> Result<()> {
    let args = Args::parse();
    let markdown = read_input(&args.file)?;

    let preprocessed = preprocess(create_markdown_parser(&markdown), args.highlight_command)?;
    let title = args.title.or_else(|| {
        preprocessed
            .title_events
            .map(|events| to_plain_text(events.into_iter()))
    });
    let body_html = to_html(preprocessed.events.into_iter());
    let html = render_template(
        title.as_deref(),
        args.metadata,
        preprocessed.has_math,
        preprocessed.has_highlighted_code,
        &preprocessed.metadata,
        &body_html,
        &args.template,
    )?
    .unwrap_or(body_html);
    write_output(&output_file_name(&args.file, args.output), &html)?;
    Ok(())
}

fn to_html<'a>(events: impl Iterator<Item = pulldown_cmark::Event<'a>>) -> String {
    let mut html = String::new();
    pulldown_cmark::html::push_html(&mut html, events);
    html
}

fn render_template(
    title: Option<&str>,
    metadata: Option<Json>,
    math: bool,
    has_highlighted_code: bool,
    frontmatter: &Option<json::Value>,
    content: &str,
    template_files: &[String],
) -> Result<Option<String>> {
    let mut tera = tera::Tera::default();
    tera.add_template_files(template_files.iter().map(|f| (f, None::<&str>)))?;
    let mut context = tera::Context::new();
    context.insert("title", &title);
    context.insert("content", content);
    context.insert("metadata", &metadata.map(|m| m.0));
    if let Some(frontmatter) = frontmatter {
        context.insert("frontmatter", &frontmatter);
    }
    context.insert("math", &math);
    context.insert("has_highlighted_code", &has_highlighted_code);

    if let Some(template) = template_files.first() {
        Ok(Some(tera.render(template, &context)?))
    } else {
        Ok(None)
    }
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
    if Path::new(path).exists() {
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
