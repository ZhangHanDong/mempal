pub fn build_filter_clause(alias: &str, start_param: usize) -> String {
    let prefix = if alias.is_empty() {
        String::new()
    } else {
        format!("{alias}.")
    };
    let wing_param = start_param;
    let room_param = start_param + 1;
    let memory_kind_param = start_param + 2;
    let domain_param = start_param + 3;
    let field_param = start_param + 4;
    let tier_param = start_param + 5;
    let status_param = start_param + 6;
    let anchor_kind_param = start_param + 7;

    format!(
        "WHERE {prefix}deleted_at IS NULL \
         AND (?{wing_param} IS NULL OR {prefix}wing = ?{wing_param}) \
         AND (?{room_param} IS NULL OR {prefix}room = ?{room_param}) \
         AND (?{memory_kind_param} IS NULL OR {prefix}memory_kind = ?{memory_kind_param}) \
         AND (?{domain_param} IS NULL OR {prefix}domain = ?{domain_param}) \
         AND (?{field_param} IS NULL OR {prefix}field = ?{field_param}) \
         AND (?{tier_param} IS NULL OR {prefix}tier = ?{tier_param}) \
         AND (?{status_param} IS NULL OR {prefix}status = ?{status_param}) \
         AND (?{anchor_kind_param} IS NULL OR {prefix}anchor_kind = ?{anchor_kind_param})"
    )
}
