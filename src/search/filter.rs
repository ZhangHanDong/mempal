pub fn build_filter_clause(alias: &str, wing_param: usize, room_param: usize) -> String {
    let prefix = if alias.is_empty() {
        String::new()
    } else {
        format!("{alias}.")
    };

    format!(
        "WHERE {prefix}deleted_at IS NULL \
         AND (?{wing_param} IS NULL OR {prefix}wing = ?{wing_param}) \
         AND (?{room_param} IS NULL OR {prefix}room = ?{room_param})"
    )
}
