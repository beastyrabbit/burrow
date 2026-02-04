use console::{style, Term};
use serde::Serialize;

pub fn print_success(msg: &str) {
    let term = Term::stdout();
    let _ = term.write_line(&format!("{} {}", style("✓").green().bold(), msg));
}

pub fn print_error(msg: &str) {
    let term = Term::stderr();
    let _ = term.write_line(&format!("{} {}", style("✗").red().bold(), msg));
}

pub fn print_warning(msg: &str) {
    let term = Term::stdout();
    let _ = term.write_line(&format!("{} {}", style("!").yellow().bold(), msg));
}

pub fn print_info(msg: &str) {
    let term = Term::stdout();
    let _ = term.write_line(&format!("{} {}", style("•").cyan(), msg));
}

pub fn print_heading(msg: &str) {
    let term = Term::stdout();
    let _ = term.write_line(&style(msg).bold().underlined().to_string());
}

/// Print a key-value pair with colored status
pub fn print_status(label: &str, ok: bool) {
    let term = Term::stdout();
    let status = if ok {
        style("OK").green().bold()
    } else {
        style("FAIL").red().bold()
    };
    let _ = term.write_line(&format!("  {}: {}", style(label).dim(), status));
}

/// Print a key-value pair
pub fn print_kv(key: &str, value: &str) {
    let term = Term::stdout();
    let _ = term.write_line(&format!("  {}: {}", style(key).dim(), value));
}

/// Print output as JSON
pub fn print_json<T: Serialize>(data: &T) -> Result<(), String> {
    let json = serde_json::to_string_pretty(data).map_err(|e| e.to_string())?;
    println!("{json}");
    Ok(())
}

/// Print output as compact JSON (single line)
pub fn print_json_compact<T: Serialize>(data: &T) -> Result<(), String> {
    let json = serde_json::to_string(data).map_err(|e| e.to_string())?;
    println!("{json}");
    Ok(())
}

#[cfg(test)]
mod tests {
    use serde::Serialize;

    #[derive(Serialize)]
    struct TestData {
        foo: String,
        bar: i32,
    }

    #[test]
    fn json_output_works() {
        let data = TestData {
            foo: "hello".into(),
            bar: 42,
        };
        // Just test that it doesn't panic
        let result = serde_json::to_string_pretty(&data);
        assert!(result.is_ok());
        let json = result.unwrap();
        assert!(json.contains("hello"));
        assert!(json.contains("42"));
    }

    #[test]
    fn compact_json_is_single_line() {
        let data = TestData {
            foo: "test".into(),
            bar: 1,
        };
        let json = serde_json::to_string(&data).unwrap();
        assert!(!json.contains('\n'));
    }
}
