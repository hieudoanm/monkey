use monkey_project::jungle;
use monkey_project::manifest;
use monkey_project::project;
use std::fs;
use std::path::PathBuf;

fn temp_dir(name: &str) -> PathBuf {
    let dir = std::env::temp_dir().join(format!("monkey-project-test-{}", name));
    let _ = fs::remove_dir_all(&dir);
    fs::create_dir_all(&dir).unwrap();
    dir
}

#[test]
fn jungle_parses_keys_comments_quotes() {
    let src = r#"
# comment line
project.manifest = manifest.xml
base.sourcePath = source/**/*.mc
quoted = "hello world"
single = 'it''s'
no-equals-line
bad =
"#;
    let j = jungle::parse(src);
    assert_eq!(j.get("project.manifest"), Some("manifest.xml"));
    assert_eq!(j.get("base.sourcePath"), Some("source/**/*.mc"));
    assert_eq!(j.get("quoted"), Some("hello world"));
    assert_eq!(j.get("single"), Some("it''s"));
    assert_eq!(j.get("bad"), Some(""));
    assert_eq!(j.entries.len(), 5);
}

#[test]
fn jungle_duplicate_keys_last_wins() {
    let j = jungle::parse("a = 1\na = 2\n");
    assert_eq!(j.get("a"), Some("2"));
}

#[test]
fn manifest_parses_application() {
    let src = r#"<?xml version="1.0"?>
<iq:manifest xmlns:iq="http://www.garmin.com/xml/connectiq">
  <iq:application id="abc-123" type="watchface" name="@Strings.AppName"
      entry="MyApp" launcherIcon="@Drawables.Icon" minApiLevel="3.0.0">
    <iq:products>
      <iq:product id="fr55"/>
      <iq:product id="venu3"/>
    </iq:products>
    <iq:permissions>
      <iq:uses-permission id="Communications"/>
    </iq:permissions>
    <iq:languages><iq:language>eng</iq:language></iq:languages>
  </iq:application>
</iq:manifest>"#;
    let m = manifest::parse(src);
    let app = m.application.expect("application element");
    assert_eq!(app.id.as_deref(), Some("abc-123"));
    assert_eq!(app.app_type.as_deref(), Some("watchface"));
    assert!(app.is_watchface());
    assert_eq!(app.entry.as_deref(), Some("MyApp"));
    assert_eq!(app.min_api_level.as_deref(), Some("3.0.0"));
    assert_eq!(app.products, vec!["fr55", "venu3"]);
    assert_eq!(app.permissions, vec!["Communications"]);
    assert_eq!(app.languages, vec!["eng"]);
}

#[test]
fn manifest_missing_application_is_empty() {
    let m = manifest::parse("<iq:manifest></iq:manifest>");
    assert!(m.application.is_none());
}

#[test]
fn discover_finds_project_from_nested_file() {
    let root = temp_dir("discover");
    let app_dir = root.join("apps/hands");
    let src_dir = app_dir.join("source");
    fs::create_dir_all(&src_dir).unwrap();
    fs::write(
        app_dir.join("monkey.jungle"),
        "project.manifest = manifest.xml\n",
    )
    .unwrap();
    fs::write(
        app_dir.join("manifest.xml"),
        r#"<iq:manifest><iq:application type="watchface" name="T"><iq:products><iq:product id="fr55"/></iq:products></iq:application></iq:manifest>"#,
    )
    .unwrap();
    fs::write(src_dir.join("App.mc"), "class App {}\n").unwrap();
    fs::create_dir_all(app_dir.join("resources")).unwrap();

    let file = src_dir.join("App.mc");
    let p = project::discover(&file).expect("project discovered");
    assert_eq!(p.root, app_dir);
    assert_eq!(p.describe(), "T (watchface) → fr55");
    assert!(project::check_files(&p).is_empty());

    let _ = fs::remove_dir_all(&root);
}

#[test]
fn discover_returns_none_outside_project() {
    let dir = temp_dir("outside");
    fs::write(dir.join("random.txt"), "hi").unwrap();
    assert!(project::discover(&dir).is_none());
    let _ = fs::remove_dir_all(&dir);
}

#[test]
fn source_dir_handles_globs() {
    let root = temp_dir("srcdir");
    fs::write(
        root.join("monkey.jungle"),
        "project.manifest = manifest.xml\nbase.sourcePath = src/**/*.mc\n",
    )
    .unwrap();
    fs::write(root.join("manifest.xml"), "<iq:manifest/>").unwrap();
    let p = project::load(&root).unwrap();
    assert_eq!(p.source_dir(), root.join("src"));
    let _ = fs::remove_dir_all(&root);
}
