use std::collections::BTreeMap;

pub type TokenDelta = BTreeMap<String, Option<String>>;

#[must_use]
pub fn chunks(tokens: &TokenDelta) -> Vec<TokenDelta> {
    let entries: Vec<_> = tokens.iter().collect();
    entries
        .chunks(16)
        .map(|chunk| {
            chunk
                .iter()
                .map(|(key, value)| ((*key).clone(), (*value).clone()))
                .collect()
        })
        .collect()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn metadata_is_chunked_at_api_limit() {
        let tokens = (0..33)
            .map(|index| (format!("token_{index}"), Some(index.to_string())))
            .collect();
        let chunks = chunks(&tokens);
        assert_eq!(
            chunks.iter().map(BTreeMap::len).collect::<Vec<_>>(),
            [16, 16, 1]
        );
    }
}
