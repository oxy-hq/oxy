use serde::Deserialize;

#[derive(Debug, Clone, Deserialize)]
pub(super) struct ContextHeader {
    pub(super) oxy: OxyHeaderData,
}

#[derive(Debug, Clone, Deserialize)]
#[serde(untagged)]
pub(super) enum Embed {
    String(String),
    Multiple(Vec<String>),
}

#[derive(Debug, Clone, Deserialize)]
pub(super) struct OxyHeaderData {
    pub(super) embed: Embed,
}

// example format
// /*
// oxy:
//     embed: |
//         this return fruit with sales
//         fruit including apple, banana, kiwi, cherry and orange
// */
// select 'apple' as name, 325 as sales
// union all
// select 'banana' as name, 2000 as sales
// union all
// select 'cherry' as name, 18 as sales
// union all
// select 'kiwi' as name, 120 as sales
// union all
// select 'orange' as name, 1500 as sales
pub(super) fn parse_embed_document(content: &str) -> Option<(String, ContextHeader)> {
    let context_regex = regex::Regex::new(r"(?m)^\/\*((?:.|\n)+)\*\/((.|\n)+)$").unwrap();
    let context_match = context_regex.captures(content);
    context_match.as_ref()?;
    let context_match = context_match.unwrap();
    let comment_content = context_match[1].replace("\n*", "\n");
    let context_content = context_match[2].to_string();
    let header_data: Result<ContextHeader, serde_yaml::Error> =
        serde_yaml::from_str(comment_content.as_str());
    if header_data.is_err() {
        log::warn!(
            "Failed to parse header data: {:?}, error: {:?}",
            comment_content,
            header_data
        );
        return None;
    }

    let header_data = header_data.unwrap();
    Some((context_content.trim().to_owned(), header_data))
}
