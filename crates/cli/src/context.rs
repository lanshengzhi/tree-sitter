use std::{
    fs,
    io::{self, Write},
    path::Path,
};

use anyhow::{Context, Result, anyhow};
use tree_sitter::Parser;
use tree_sitter_context::{chunk::chunks_for_tree, schema::ContextOutput};
use tree_sitter_loader::Loader;

pub struct ContextOptions {
    pub quiet: bool,
}

pub fn run(loader: &mut Loader, path: &Path, _opts: &ContextOptions) -> Result<()> {
    let source = fs::read(path).with_context(|| format!("failed to read {}", path.display()))?;

    let language = loader
        .language_configuration_for_file_name(path)?
        .map(|(lang, _)| lang)
        .ok_or_else(|| anyhow!("no language found for {}", path.display()))?;

    let mut parser = Parser::new();
    parser.set_language(&language)?;

    let tree = parser
        .parse(&source, None)
        .ok_or_else(|| anyhow!("failed to parse {}", path.display()))?;

    let mut output = ContextOutput::new("0.1.0").with_source_path(path);

    let chunks = chunks_for_tree(&tree, path, &source, &Default::default());
    for chunk in chunks {
        output.push_chunk(chunk);
    }

    if output.meta.total_chunks == 0 {
        output.push_diagnostic(tree_sitter_context::schema::Diagnostic::warn(
            "no chunk boundaries found; file may be empty or use an unsupported grammar",
        ));
    }

    let stdout = io::stdout();
    let mut stdout = stdout.lock();
    writeln!(&mut stdout, "{}", serde_json::to_string_pretty(&output)?)?;

    Ok(())
}
