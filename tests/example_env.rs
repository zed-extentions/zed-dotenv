#![allow(dead_code, unused_imports)]

#[path = "../src/language_server/assets.rs"]
mod assets;
#[path = "../src/language_server/completion.rs"]
mod completion;
#[path = "../src/audit/drift.rs"]
mod drift;
#[path = "../src/http/mod.rs"]
mod http;
#[path = "../src/audit/parser.rs"]
mod parser;
#[path = "../src/audit/paths.rs"]
mod paths;
#[path = "../src/reports/mod.rs"]
mod report;
#[path = "../src/source_scan/mod.rs"]
mod scan;
#[path = "../src/security/mod.rs"]
mod secrets;
#[path = "../src/language_server/settings.rs"]
mod settings;
#[path = "../src/audit/types.rs"]
mod types;

pub use assets::*;
pub use completion::*;
pub use drift::*;
pub use http::*;
pub use parser::*;
pub use paths::*;
pub use report::*;
pub use scan::*;
pub use secrets::*;
pub use settings::*;
pub use types::*;

#[test]
fn example_env_fixture_parses_without_errors() {
    let content = include_str!("fixtures/example.env");
    let report = parse_dotenv(content, false);

    assert!(
        report.diagnostics.is_empty(),
        "example.env should be valid, got diagnostics: {:?}",
        report.diagnostics
    );
}
