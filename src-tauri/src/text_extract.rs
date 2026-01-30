use std::io::Read;
use std::path::Path;

pub fn extract_text(path: &Path, max_chars: usize) -> Result<String, String> {
    let ext = path
        .extension()
        .and_then(|e| e.to_str())
        .unwrap_or("");
    let text = match ext.to_lowercase().as_str() {
        "txt" | "md" | "rs" | "ts" | "tsx" | "js" | "py" | "toml" | "yaml" | "yml" | "json"
        | "sh" | "css" | "html" | "csv" | "rtf" => read_text_file(path, max_chars),
        "pdf" => extract_pdf(path, max_chars),
        "docx" => extract_docx(path, max_chars),
        "xlsx" | "xls" | "ods" => extract_spreadsheet(path, max_chars),
        "pptx" => extract_pptx(path, max_chars),
        "odt" | "odp" => extract_odf(path, max_chars),
        _ => Err(format!("Unsupported format: {ext}")),
    }?;
    Ok(text)
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
        .filter_map(|i| {
            let name = archive.by_index(i).ok()?.name().to_string();
            if name.starts_with("ppt/slides/slide") && name.ends_with(".xml") {
                Some(name)
            } else {
                None
            }
        })
        .collect();
    slide_names.sort();

    let mut result = String::new();
    for name in &slide_names {
        if let Ok(mut entry) = archive.by_name(name) {
            let mut xml = String::new();
            if entry.read_to_string(&mut xml).is_ok() {
                let text = strip_xml_tags(&xml);
                if !text.is_empty() {
                    if !result.is_empty() {
                        result.push('\n');
                    }
                    result.push_str(&text);
                    if result.len() >= max_chars {
                        return Ok(result.chars().take(max_chars).collect());
                    }
                }
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
    for xml_path in xml_paths {
        if let Ok(mut entry) = archive.by_name(xml_path) {
            let mut xml = String::new();
            entry.read_to_string(&mut xml).map_err(|e| e.to_string())?;
            let text = strip_xml_tags(&xml);
            if !result.is_empty() && !text.is_empty() {
                result.push('\n');
            }
            result.push_str(&text);
            if result.len() >= max_chars {
                return Ok(result.chars().take(max_chars).collect());
            }
        }
    }

    if result.is_empty() {
        Err(format!("No text content found in {}", path.display()))
    } else {
        Ok(result)
    }
}

fn extract_spreadsheet(path: &Path, max_chars: usize) -> Result<String, String> {
    use calamine::{open_workbook_auto, Data, Reader};

    let mut workbook = open_workbook_auto(path).map_err(|e| e.to_string())?;
    let sheet_names: Vec<String> = workbook.sheet_names().to_vec();
    let mut result = String::new();

    for name in &sheet_names {
        if let Ok(range) = workbook.worksheet_range(name) {
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
                    }
                    result.push_str(&line);
                    if result.len() >= max_chars {
                        return Ok(result.chars().take(max_chars).collect());
                    }
                }
            }
        }
    }

    if result.is_empty() {
        Err(format!("No text content found in {}", path.display()))
    } else {
        Ok(result)
    }
}

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
    let normalized: String = result.split_whitespace().collect::<Vec<_>>().join(" ");
    normalized
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::fs;
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
        assert_eq!(
            strip_xml_tags(r#"<p class="x">text</p>"#),
            "text"
        );
    }

    #[test]
    fn strip_xml_whitespace_normalization() {
        assert_eq!(
            strip_xml_tags("<p>  hello   world  </p>"),
            "hello world"
        );
    }

    #[test]
    fn strip_xml_empty() {
        assert_eq!(strip_xml_tags("<br/><hr/>"), "");
    }

    #[test]
    fn extract_no_extension() {
        let tmp = TempDir::new().unwrap();
        let file = tmp.path().join("Makefile");
        fs::write(&file, "all:").unwrap();
        assert!(extract_text(&file, 1000).is_err());
    }
}
