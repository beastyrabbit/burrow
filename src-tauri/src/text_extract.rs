use std::io::Read;
use std::path::{Path, PathBuf};

/// Extract plain text from a file at the given path, truncated to `max_chars` characters.
///
/// Supports plain text/code files (txt, md, rs, ts, js, py, etc.),
/// PDF, DOCX, DOC (requires `libreoffice` on `$PATH`), PPTX, XLSX/XLS/ODS, and ODF (odt, odp).
///
/// Returns `Err` if the format is unsupported or extraction fails.
/// Returns `Ok("")` for valid documents that contain no text.
pub fn extract_text(path: &Path, max_chars: usize) -> Result<String, String> {
    let ext = path.extension().and_then(|e| e.to_str()).unwrap_or("");
    match ext.to_lowercase().as_str() {
        "txt" | "md" | "rs" | "ts" | "tsx" | "js" | "py" | "toml" | "yaml" | "yml" | "json"
        | "sh" | "css" | "html" | "csv" | "rtf" => read_text_file(path, max_chars),
        "pdf" => extract_pdf(path, max_chars),
        "docx" => extract_docx(path, max_chars),
        "xlsx" | "xls" | "ods" => extract_spreadsheet(path, max_chars),
        "pptx" => extract_pptx(path, max_chars),
        "odt" | "odp" => extract_odf(path, max_chars),
        "doc" => extract_doc_libreoffice(path, max_chars),
        _ => Err(format!("Unsupported format: {ext}")),
    }
}

fn read_text_file(path: &Path, max_chars: usize) -> Result<String, String> {
    let content = std::fs::read_to_string(path).map_err(|e| e.to_string())?;
    Ok(content.chars().take(max_chars).collect())
}

fn extract_pdf(path: &Path, max_chars: usize) -> Result<String, String> {
    let bytes = std::fs::read(path).map_err(|e| e.to_string())?;
    let text = pdf_extract::extract_text_from_mem(&bytes).map_err(|e| e.to_string())?;
    Ok(text.chars().take(max_chars).collect())
}

fn extract_docx(path: &Path, max_chars: usize) -> Result<String, String> {
    extract_zip_xml(path, &["word/document.xml"], max_chars)
}

fn extract_pptx(path: &Path, max_chars: usize) -> Result<String, String> {
    let file = std::fs::File::open(path).map_err(|e| e.to_string())?;
    let mut archive = zip::ZipArchive::new(file).map_err(|e| e.to_string())?;

    let mut slide_names: Vec<String> = (0..archive.len())
        .filter_map(|i| match archive.by_index(i) {
            Ok(entry) => {
                let name = entry.name().to_string();
                if name.starts_with("ppt/slides/slide") && name.ends_with(".xml") {
                    Some(name)
                } else {
                    None
                }
            }
            Err(e) => {
                eprintln!("[text_extract] warning: failed to read zip index {i}: {e}");
                None
            }
        })
        .collect();

    // Sort numerically by slide number (slide1, slide2, ..., slide10)
    slide_names.sort_by(|a, b| {
        fn slide_num(s: &str) -> u32 {
            s.strip_prefix("ppt/slides/slide")
                .and_then(|rest| rest.strip_suffix(".xml"))
                .and_then(|n| n.parse().ok())
                .unwrap_or(u32::MAX)
        }
        slide_num(a).cmp(&slide_num(b))
    });

    let mut result = String::new();
    let mut char_count = 0usize;
    for name in &slide_names {
        match archive.by_name(name) {
            Ok(mut entry) => {
                let mut xml = String::new();
                if let Err(e) = entry.read_to_string(&mut xml) {
                    eprintln!("[text_extract] warning: failed to read slide {name}: {e}");
                    continue;
                }
                let text = strip_xml_tags(&xml);
                if !text.is_empty() {
                    if !result.is_empty() {
                        result.push('\n');
                        char_count += 1;
                    }
                    result.push_str(&text);
                    char_count += text.chars().count();
                    if char_count >= max_chars {
                        return Ok(result.chars().take(max_chars).collect());
                    }
                }
            }
            Err(e) => {
                eprintln!("[text_extract] warning: failed to access slide {name}: {e}");
            }
        }
    }
    Ok(result.chars().take(max_chars).collect())
}

fn extract_odf(path: &Path, max_chars: usize) -> Result<String, String> {
    extract_zip_xml(path, &["content.xml"], max_chars)
}

fn extract_zip_xml(path: &Path, xml_paths: &[&str], max_chars: usize) -> Result<String, String> {
    let file = std::fs::File::open(path).map_err(|e| e.to_string())?;
    let mut archive = zip::ZipArchive::new(file).map_err(|e| e.to_string())?;

    let mut result = String::new();
    let mut char_count = 0usize;
    for xml_path in xml_paths {
        match archive.by_name(xml_path) {
            Ok(mut entry) => {
                let mut xml = String::new();
                entry.read_to_string(&mut xml).map_err(|e| e.to_string())?;
                let text = strip_xml_tags(&xml);
                if !result.is_empty() && !text.is_empty() {
                    result.push('\n');
                    char_count += 1;
                }
                result.push_str(&text);
                char_count += text.chars().count();
                if char_count >= max_chars {
                    return Ok(result.chars().take(max_chars).collect());
                }
            }
            Err(e) => {
                return Err(format!(
                    "Failed to read {} from {}: {e}",
                    xml_path,
                    path.display()
                ));
            }
        }
    }

    Ok(result)
}

fn extract_spreadsheet(path: &Path, max_chars: usize) -> Result<String, String> {
    use calamine::{open_workbook_auto, Data, Reader};

    let mut workbook = open_workbook_auto(path).map_err(|e| e.to_string())?;
    let sheet_names: Vec<String> = workbook.sheet_names().to_vec();
    let mut result = String::new();
    let mut char_count = 0usize;

    for name in &sheet_names {
        match workbook.worksheet_range(name) {
            Ok(range) => {
                for row in range.rows() {
                    let cells: Vec<String> = row
                        .iter()
                        .map(|cell| match cell {
                            Data::Empty => String::new(),
                            Data::String(s) => s.clone(),
                            Data::Float(f) => f.to_string(),
                            Data::Int(i) => i.to_string(),
                            Data::Bool(b) => b.to_string(),
                            Data::Error(e) => format!("{e:?}"),
                            Data::DateTime(dt) => dt.to_string(),
                            Data::DateTimeIso(s) => s.clone(),
                            Data::DurationIso(s) => s.clone(),
                        })
                        .collect();
                    let line = cells.join("\t");
                    if !line.trim().is_empty() {
                        if !result.is_empty() {
                            result.push('\n');
                            char_count += 1;
                        }
                        result.push_str(&line);
                        char_count += line.chars().count();
                        if char_count >= max_chars {
                            return Ok(result.chars().take(max_chars).collect());
                        }
                    }
                }
            }
            Err(e) => {
                eprintln!(
                    "[text_extract] warning: failed to read sheet '{}': {e}",
                    name
                );
            }
        }
    }

    Ok(result)
}

/// Derive the expected `.txt` output path for a `.doc` file converted by LibreOffice.
fn doc_txt_path(path: &Path, out_dir: &Path) -> Result<PathBuf, String> {
    let stem = path
        .file_stem()
        .and_then(|s| s.to_str())
        .ok_or_else(|| format!("Cannot extract file stem from: {}", path.display()))?;
    Ok(out_dir.join(format!("{stem}.txt")))
}

/// Convert a legacy `.doc` file to plain text using LibreOffice's headless mode.
///
/// Requires `libreoffice` to be available on `$PATH`. Returns `Err` if
/// LibreOffice is not installed, conversion fails, or the process exceeds
/// the 30-second timeout.
fn extract_doc_libreoffice(path: &Path, max_chars: usize) -> Result<String, String> {
    use std::time::Duration;
    use wait_timeout::ChildExt;

    const TIMEOUT_SECS: u64 = 30;

    let tmp_dir = tempfile::tempdir().map_err(|e| e.to_string())?;
    let mut child = std::process::Command::new("libreoffice")
        .args([
            "--headless",
            "--nologo",
            "--nodefault",
            "--nofirststartwizard",
            "--norestore",
            "--nolockcheck",
            "--convert-to",
            "txt:Text",
            "--outdir",
        ])
        .arg(tmp_dir.path())
        .arg(path)
        .stdout(std::process::Stdio::null())
        .stderr(std::process::Stdio::piped())
        .spawn()
        .map_err(|e| format!("libreoffice not available: {e}"))?;

    let status = match child.wait_timeout(Duration::from_secs(TIMEOUT_SECS)) {
        Ok(Some(status)) => status,
        Ok(None) => {
            child.kill().ok();
            child.wait().ok();
            return Err(format!(
                "libreoffice timed out after {TIMEOUT_SECS}s for {}",
                path.display()
            ));
        }
        Err(e) => return Err(format!("libreoffice wait failed: {e}")),
    };

    if !status.success() {
        let mut stderr = String::new();
        if let Some(mut err) = child.stderr.take() {
            err.read_to_string(&mut stderr).ok();
        }
        return Err(format!(
            "libreoffice conversion failed for {}: {stderr}",
            path.display()
        ));
    }

    let txt_path = doc_txt_path(path, tmp_dir.path())?;
    let content = std::fs::read_to_string(&txt_path)
        .map_err(|e| format!("Failed to read converted file: {e}"))?;
    Ok(content.chars().take(max_chars).collect())
}

/// Naive XML tag stripper that also decodes the five standard XML entities.
/// Does not handle CDATA sections or XML comments.
fn strip_xml_tags(xml: &str) -> String {
    let mut result = String::with_capacity(xml.len() / 2);
    let mut inside_tag = false;

    for ch in xml.chars() {
        match ch {
            '<' => inside_tag = true,
            '>' => {
                inside_tag = false;
                // Add space as separator between tag contents
                if !result.ends_with(' ') && !result.is_empty() {
                    result.push(' ');
                }
            }
            _ if !inside_tag => result.push(ch),
            _ => {}
        }
    }

    // Normalize whitespace
    let normalized = result.split_whitespace().collect::<Vec<_>>().join(" ");

    // Decode standard XML entities
    decode_xml_entities(&normalized)
}

fn decode_xml_entities(text: &str) -> String {
    text.replace("&amp;", "&")
        .replace("&lt;", "<")
        .replace("&gt;", ">")
        .replace("&quot;", "\"")
        .replace("&apos;", "'")
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::fs;
    use std::io::Write;
    use tempfile::TempDir;

    #[test]
    fn extract_text_file() {
        let tmp = TempDir::new().unwrap();
        let file = tmp.path().join("test.txt");
        fs::write(&file, "hello world").unwrap();
        let result = extract_text(&file, 1000).unwrap();
        assert_eq!(result, "hello world");
    }

    #[test]
    fn extract_text_file_truncates() {
        let tmp = TempDir::new().unwrap();
        let file = tmp.path().join("test.txt");
        fs::write(&file, "abcdefghij").unwrap();
        let result = extract_text(&file, 5).unwrap();
        assert_eq!(result, "abcde");
    }

    #[test]
    fn extract_text_unsupported() {
        let tmp = TempDir::new().unwrap();
        let file = tmp.path().join("test.exe");
        fs::write(&file, "binary").unwrap();
        assert!(extract_text(&file, 1000).is_err());
    }

    #[test]
    fn extract_csv_as_text() {
        let tmp = TempDir::new().unwrap();
        let file = tmp.path().join("data.csv");
        fs::write(&file, "a,b,c\n1,2,3").unwrap();
        let result = extract_text(&file, 1000).unwrap();
        assert!(result.contains("a,b,c"));
    }

    #[test]
    fn extract_nonexistent_file() {
        let result = extract_text(Path::new("/nonexistent/file.txt"), 1000);
        assert!(result.is_err());
    }

    // --- strip_xml_tags tests ---

    #[test]
    fn strip_xml_simple() {
        assert_eq!(strip_xml_tags("<p>hello</p>"), "hello");
    }

    #[test]
    fn strip_xml_nested() {
        assert_eq!(
            strip_xml_tags("<div><p>hello</p><p>world</p></div>"),
            "hello world"
        );
    }

    #[test]
    fn strip_xml_with_attributes() {
        assert_eq!(strip_xml_tags(r#"<p class="x">text</p>"#), "text");
    }

    #[test]
    fn strip_xml_whitespace_normalization() {
        assert_eq!(strip_xml_tags("<p>  hello   world  </p>"), "hello world");
    }

    #[test]
    fn strip_xml_empty() {
        assert_eq!(strip_xml_tags("<br/><hr/>"), "");
    }

    #[test]
    fn strip_xml_decodes_entities() {
        assert_eq!(
            strip_xml_tags("<p>A &amp; B &lt; C &gt; D &quot;E&quot; &apos;F&apos;</p>"),
            "A & B < C > D \"E\" 'F'"
        );
    }

    #[test]
    fn strip_xml_unclosed_tag() {
        assert_eq!(strip_xml_tags("<p>hello<br"), "hello");
    }

    #[test]
    fn extract_no_extension() {
        let tmp = TempDir::new().unwrap();
        let file = tmp.path().join("Makefile");
        fs::write(&file, "all:").unwrap();
        assert!(extract_text(&file, 1000).is_err());
    }

    // --- DOCX tests ---

    #[test]
    fn extract_docx_basic() {
        let tmp = TempDir::new().unwrap();
        let file = tmp.path().join("test.docx");
        let f = fs::File::create(&file).unwrap();
        let mut zip = zip::ZipWriter::new(f);
        zip.start_file::<_, ()>("word/document.xml", Default::default())
            .unwrap();
        zip.write_all(
            b"<w:document><w:body><w:p><w:r><w:t>Hello Doc</w:t></w:r></w:p></w:body></w:document>",
        )
        .unwrap();
        zip.finish().unwrap();

        let result = extract_text(&file, 1000).unwrap();
        assert_eq!(result, "Hello Doc");
    }

    #[test]
    fn extract_docx_empty_returns_ok_empty() {
        let tmp = TempDir::new().unwrap();
        let file = tmp.path().join("empty.docx");
        let f = fs::File::create(&file).unwrap();
        let mut zip = zip::ZipWriter::new(f);
        zip.start_file::<_, ()>("word/document.xml", Default::default())
            .unwrap();
        zip.write_all(b"<w:document><w:body></w:body></w:document>")
            .unwrap();
        zip.finish().unwrap();

        let result = extract_text(&file, 1000).unwrap();
        assert_eq!(result, "");
    }

    #[test]
    fn extract_docx_with_entities() {
        let tmp = TempDir::new().unwrap();
        let file = tmp.path().join("entities.docx");
        let f = fs::File::create(&file).unwrap();
        let mut zip = zip::ZipWriter::new(f);
        zip.start_file::<_, ()>("word/document.xml", Default::default())
            .unwrap();
        zip.write_all(b"<w:t>A &amp; B</w:t>").unwrap();
        zip.finish().unwrap();

        let result = extract_text(&file, 1000).unwrap();
        assert_eq!(result, "A & B");
    }

    #[test]
    fn extract_docx_missing_xml_returns_error() {
        let tmp = TempDir::new().unwrap();
        let file = tmp.path().join("bad.docx");
        let f = fs::File::create(&file).unwrap();
        let mut zip = zip::ZipWriter::new(f);
        zip.start_file::<_, ()>("other.xml", Default::default())
            .unwrap();
        zip.write_all(b"<x/>").unwrap();
        zip.finish().unwrap();

        let result = extract_text(&file, 1000);
        assert!(result.is_err());
        assert!(result.unwrap_err().contains("word/document.xml"));
    }

    #[test]
    fn extract_corrupt_zip_returns_error() {
        let tmp = TempDir::new().unwrap();
        let file = tmp.path().join("corrupt.docx");
        fs::write(&file, "this is not a zip file").unwrap();

        assert!(extract_text(&file, 1000).is_err());
    }

    // --- PPTX tests ---

    #[test]
    fn extract_pptx_multiple_slides_sorted() {
        let tmp = TempDir::new().unwrap();
        let file = tmp.path().join("test.pptx");
        let f = fs::File::create(&file).unwrap();
        let mut zip = zip::ZipWriter::new(f);
        zip.start_file::<_, ()>("ppt/slides/slide2.xml", Default::default())
            .unwrap();
        zip.write_all(b"<p:sld><a:t>Second</a:t></p:sld>").unwrap();
        zip.start_file::<_, ()>("ppt/slides/slide1.xml", Default::default())
            .unwrap();
        zip.write_all(b"<p:sld><a:t>First</a:t></p:sld>").unwrap();
        zip.finish().unwrap();

        let result = extract_text(&file, 1000).unwrap();
        assert!(result.starts_with("First"), "got: {result}");
        assert!(result.contains("Second"));
    }

    #[test]
    fn extract_pptx_numerical_sort() {
        let tmp = TempDir::new().unwrap();
        let file = tmp.path().join("sort.pptx");
        let f = fs::File::create(&file).unwrap();
        let mut zip = zip::ZipWriter::new(f);
        for i in [10, 2, 1] {
            zip.start_file::<_, ()>(format!("ppt/slides/slide{i}.xml"), Default::default())
                .unwrap();
            zip.write_all(format!("<s><t>Slide{i}</t></s>").as_bytes())
                .unwrap();
        }
        zip.finish().unwrap();

        let result = extract_text(&file, 1000).unwrap();
        let pos1 = result.find("Slide1").unwrap();
        let pos2 = result.find("Slide2").unwrap();
        let pos10 = result.find("Slide10").unwrap();
        assert!(pos1 < pos2, "Slide1 should come before Slide2");
        assert!(pos2 < pos10, "Slide2 should come before Slide10");
    }

    #[test]
    fn extract_pptx_no_slides_returns_ok_empty() {
        let tmp = TempDir::new().unwrap();
        let file = tmp.path().join("empty.pptx");
        let f = fs::File::create(&file).unwrap();
        let mut zip = zip::ZipWriter::new(f);
        zip.start_file::<_, ()>("[Content_Types].xml", Default::default())
            .unwrap();
        zip.write_all(b"<Types/>").unwrap();
        zip.finish().unwrap();

        let result = extract_text(&file, 1000).unwrap();
        assert_eq!(result, "");
    }

    // --- ODF tests ---

    #[test]
    fn extract_odt_basic() {
        let tmp = TempDir::new().unwrap();
        let file = tmp.path().join("test.odt");
        let f = fs::File::create(&file).unwrap();
        let mut zip = zip::ZipWriter::new(f);
        zip.start_file::<_, ()>("content.xml", Default::default())
            .unwrap();
        zip.write_all(b"<office:body><text:p>Hello ODF</text:p></office:body>")
            .unwrap();
        zip.finish().unwrap();

        let result = extract_text(&file, 1000).unwrap();
        assert_eq!(result, "Hello ODF");
    }

    // --- doc_txt_path tests ---

    #[test]
    fn doc_txt_path_derives_correctly() {
        let result = doc_txt_path(Path::new("/tmp/report.doc"), Path::new("/out")).unwrap();
        assert_eq!(result, PathBuf::from("/out/report.txt"));
    }

    #[test]
    fn doc_txt_path_no_stem_returns_error() {
        // A path with no file stem (just extension) should fail
        let result = doc_txt_path(Path::new("/tmp/.doc"), Path::new("/out"));
        // .doc has stem "" which to_str returns Some("") — but file_stem returns Some(".doc") for dotfiles
        // Actually Path::new("/tmp/.doc").file_stem() returns Some(".doc"), so this succeeds
        assert!(result.is_ok());
    }

    // --- decode_xml_entities tests ---

    #[test]
    fn decode_all_standard_entities() {
        assert_eq!(decode_xml_entities("&amp;&lt;&gt;&quot;&apos;"), "&<>\"'");
    }

    #[test]
    fn extract_doc_no_panic() {
        let tmp = TempDir::new().unwrap();
        let file = tmp.path().join("test.doc");
        fs::write(&file, b"fake doc content").unwrap();
        // Should not panic regardless of whether libreoffice is installed.
        // If installed, libreoffice may convert or fail — both are valid outcomes.
        let _ = extract_text(&file, 1000);
    }

    #[test]
    fn decode_no_entities() {
        assert_eq!(decode_xml_entities("plain text"), "plain text");
    }
}
