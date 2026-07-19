impl QueryCore {
    fn next_param_key(param_counter: &mut usize) -> String {
        let key = format!("param_{}", *param_counter);
        *param_counter += 1;
        key
    }

    fn rename_subquery_params(
        sub_query: String,
        sub_params: Vec<(String, serde_json::Value)>,
        prefix: &str,
    ) -> (String, Vec<(String, serde_json::Value)>) {
        let mut renamed = sub_query;
        let mut out_params = Vec::with_capacity(sub_params.len());
        for (k, v) in sub_params {
            let new_key = format!("{prefix}_{k}");
            renamed = renamed.replace(&format!("${k}"), &format!("${new_key}"));
            out_params.push((new_key, v));
        }
        (renamed, out_params)
    }
}
