use crate::router::SearchResult;

pub fn try_calculate(input: &str) -> Option<SearchResult> {
    let trimmed = input.trim();
    if trimmed.is_empty() {
        return None;
    }

    let has_operator = trimmed.chars().any(|c| "+-*/^%".contains(c));
    let has_fn = ["sqrt", "sin", "cos", "tan", "log", "ln", "abs"]
        .iter()
        .any(|f| trimmed.contains(f));
    let is_parens = trimmed.starts_with('(');

    if !has_operator && !has_fn && !is_parens {
        return None;
    }

    // evalexpr is a sandboxed math expression library (no code execution)
    match evalexpr::build_operator_tree(trimmed)
        .and_then(|tree| tree.eval_with_context_mut(&mut evalexpr::HashMapContext::new()))
    {
        Ok(value) => Some(SearchResult {
            id: "math-result".into(),
            name: format!("= {value}"),
            description: format!("{trimmed} = {value}"),
            icon: "".into(),
            category: "math".into(),
            exec: "".into(),
        }),
        Err(_) => None,
    }
}
