use crate::router::SearchResult;

/// Returns true if the input looks like it could be a math expression.
fn looks_like_math(input: &str) -> bool {
    let has_operator = input.chars().any(|c| "+-*/^%".contains(c));
    let has_fn = ["sqrt", "sin", "cos", "tan", "log", "ln", "abs"]
        .iter()
        .any(|f| input.contains(f));
    let is_parens = input.starts_with('(');
    has_operator || has_fn || is_parens
}

pub fn try_calculate(input: &str) -> Option<SearchResult> {
    let trimmed = input.trim();
    if trimmed.is_empty() || !looks_like_math(trimmed) {
        return None;
    }

    // mexe is a sandboxed math-only expression library (no code execution, MIT licensed)
    match mexe::eval(trimmed) {
        Ok(value) => {
            // Display integers without decimal point
            let display = if value.fract() == 0.0 && value.abs() < i64::MAX as f64 {
                format!("{}", value as i64)
            } else {
                format!("{value}")
            };
            Some(SearchResult {
                id: "math-result".into(),
                name: format!("= {display}"),
                description: format!("{trimmed} = {display}"),
                icon: "".into(),
                category: "math".into(),
                exec: "".into(),
            })
        }
        Err(_) => None,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn basic_addition() {
        let r = try_calculate("1+3").unwrap();
        assert_eq!(r.name, "= 4");
        assert_eq!(r.category, "math");
    }

    #[test]
    fn multiplication() {
        let r = try_calculate("6 * 7").unwrap();
        assert_eq!(r.name, "= 42");
    }

    #[test]
    fn division_float() {
        let r = try_calculate("10 / 3.0").unwrap();
        assert!(r.name.starts_with("= 3.3"));
    }

    #[test]
    fn exponent_not_supported() {
        // mexe doesn't support ^; returns None
        assert!(try_calculate("2^10").is_none());
    }

    #[test]
    fn parentheses() {
        let r = try_calculate("(2 + 3) * 4").unwrap();
        assert_eq!(r.name, "= 20");
    }

    #[test]
    fn nested_parens() {
        let r = try_calculate("((1 + 2) * (3 + 4))").unwrap();
        assert_eq!(r.name, "= 21");
    }

    #[test]
    fn modulo_not_supported() {
        // mexe doesn't support %; returns None
        assert!(try_calculate("17 % 5").is_none());
    }

    #[test]
    fn math_function_abs() {
        // evalexpr doesn't support abs in its default context, so the expression
        // parses but fails evaluation — our function returns None.
        let r = try_calculate("abs(-5)");
        assert!(r.is_none());
    }

    #[test]
    fn empty_input_returns_none() {
        assert!(try_calculate("").is_none());
    }

    #[test]
    fn whitespace_only_returns_none() {
        assert!(try_calculate("   ").is_none());
    }

    #[test]
    fn plain_text_returns_none() {
        assert!(try_calculate("firefox").is_none());
    }

    #[test]
    fn plain_number_returns_none() {
        assert!(try_calculate("42").is_none());
    }

    #[test]
    fn invalid_expression_returns_none() {
        assert!(try_calculate("1 ++ 2").is_none());
    }

    #[test]
    fn result_has_correct_fields() {
        let r = try_calculate("1+1").unwrap();
        assert_eq!(r.id, "math-result");
        assert_eq!(r.category, "math");
        assert!(r.exec.is_empty());
        assert!(r.icon.is_empty());
        assert!(r.description.contains("1+1"));
    }

    #[test]
    fn looks_like_math_detects_operators() {
        assert!(looks_like_math("1+2"));
        assert!(looks_like_math("3-1"));
        assert!(looks_like_math("4*5"));
        assert!(looks_like_math("6/2"));
        assert!(looks_like_math("2^3"));
        assert!(looks_like_math("7%3"));
    }

    #[test]
    fn looks_like_math_detects_functions() {
        assert!(looks_like_math("sqrt(4)"));
        assert!(looks_like_math("sin(0)"));
        assert!(looks_like_math("cos(0)"));
        assert!(looks_like_math("abs(-1)"));
    }

    #[test]
    fn looks_like_math_detects_parens() {
        assert!(looks_like_math("(1)"));
    }

    #[test]
    fn looks_like_math_rejects_plain_text() {
        assert!(!looks_like_math("firefox"));
        assert!(!looks_like_math("hello world"));
        assert!(!looks_like_math("42"));
    }
}
