use std::{
    collections::HashMap,
    fs::File,
    io::{self, BufRead},
};

use pluralizer::pluralize;
use slugify::slugify;

use crate::{config::model::FlashTextSourceType, errors::OnyxError};

use super::base::Anonymizer;

#[derive(Default, Debug, Clone)]
pub struct TrieNode {
    children: HashMap<char, TrieNode>,
    is_word_end: bool,
    case_sensitive: bool,
    replacement: Option<String>,
}

impl TrieNode {
    fn new(case_sensitive: bool) -> Self {
        TrieNode {
            children: HashMap::new(),
            is_word_end: false,
            case_sensitive,
            replacement: None,
        }
    }

    fn get_child(&self, ch: char) -> Option<&TrieNode> {
        self.children.get(&self.norm_char(ch))
    }

    fn get_or_create_child(&mut self, ch: char) -> &mut TrieNode {
        self.children
            .entry(self.norm_char(ch))
            .or_insert_with(|| TrieNode::new(self.case_sensitive))
    }

    fn norm_char(&self, ch: char) -> char {
        if self.case_sensitive {
            ch
        } else {
            ch.to_lowercase().next().unwrap()
        }
    }
}

#[derive(Debug, Clone)]
pub struct FlashTextAnonymizer {
    root: TrieNode,
    pluralize: bool,
}

impl FlashTextAnonymizer {
    pub fn new(pluralize: &bool, case_sensitive: &bool) -> Self {
        FlashTextAnonymizer {
            root: TrieNode::new(*case_sensitive),
            pluralize: *pluralize,
        }
    }

    pub fn add_keywords_file(
        &mut self,
        source: &FlashTextSourceType,
        path: &str,
    ) -> anyhow::Result<()> {
        let file = File::open(path)
            .map_err(|err| anyhow::anyhow!("Failed to open file: {:?}. Error:\n{}", path, err))?;
        io::BufReader::new(file)
            .lines()
            .try_for_each(|raw| -> anyhow::Result<()> {
                let raw = raw?;
                let line = raw.trim();
                if line.is_empty() {
                    return Ok(());
                }

                match source {
                    FlashTextSourceType::Keywords { replacement, .. } => {
                        self.add_keyword(line, replacement)?;
                    }
                    FlashTextSourceType::Mapping { delimiter, .. } => {
                        let mut parts = line.split(delimiter);
                        let word = parts.next().ok_or_else(|| {
                            anyhow::anyhow!("Failed to parse line: {}. Error: Empty line", line)
                        })?;
                        let replacement = parts.next().ok_or_else(|| {
                            anyhow::anyhow!("Failed to parse line: {}. Error: keyword and replacement need to be separated by \"{}\"", line, delimiter)
                        })?;
                        self.add_keyword(word.trim(), replacement.trim())?;
                    }
                }
                Ok(())
            })?;

        Ok(())
    }

    pub fn add_keyword(&mut self, word: &str, replacement: &str) -> anyhow::Result<()> {
        if self.pluralize {
            let singular_word = pluralize(word, 1, false);
            let plural_word = pluralize(word, 2, false);
            self.add_keyword_internal(&singular_word, replacement)?;
            self.add_keyword_internal(&plural_word, replacement)?;
        } else {
            self.add_keyword_internal(word, replacement)?;
        }
        Ok(())
    }

    pub fn replace_keywords(
        &self,
        text: &str,
        items: Option<HashMap<String, String>>,
    ) -> anyhow::Result<(String, HashMap<String, String>)> {
        let mut internal_text = text.to_string();
        internal_text.push_str("  ");
        let mut result = String::new();
        let mut ch_indices = internal_text.char_indices();
        let mut start = 0;
        let mut items: HashMap<String, String> = items.unwrap_or_default();

        while let Some((match_start, ch)) = ch_indices.next() {
            if let Some(base_replacement) = self.traverse_trie(ch, &mut ch_indices) {
                result.push_str(&internal_text[start..match_start]);
                let mut rep = base_replacement.to_string();
                start = self.skip_to_word_boundary(
                    &internal_text,
                    match_start + ch.len_utf8(),
                    &mut ch_indices,
                );
                let (item_value, addition) =
                    self.process_item_value(&internal_text[match_start..start]);
                let existing_item = items.iter().find(|(_, v)| *v == &item_value);
                match existing_item {
                    Some((k, _v)) => {
                        rep = k.to_string();
                    }
                    None => {
                        items.insert(rep.to_string(), item_value);
                    }
                }

                result.push_str(&rep);
                result.push_str(&addition);
            }
        }
        result.push_str(&internal_text[start..]);
        result = result.trim_end().to_string();

        log::info!(
            "[Anonymizer] Replaced:\n---\n{}\n---with:---\n{}",
            text,
            result
        );
        Ok((result, items))
    }

    fn add_keyword_internal(&mut self, word: &str, replacement: &str) -> anyhow::Result<()> {
        let mut node: &mut TrieNode = &mut self.root;
        for ch in word.chars() {
            node = node.get_or_create_child(ch);
        }
        node.is_word_end = true;
        node.replacement = Some(slugify!(replacement, separator = "_").to_uppercase());
        Ok(())
    }

    fn process_item_value(&self, item_value: &str) -> (String, String) {
        let final_value: String = item_value
            .trim_end_matches(|c: char| !c.is_alphabetic())
            .to_string();
        let addition = item_value[final_value.len()..].to_string();
        (final_value, addition)
    }

    fn traverse_trie<I>(&self, ch: char, ch_indices: &mut I) -> Option<String>
    where
        I: Iterator<Item = (usize, char)>,
    {
        let mut node = self.root.get_child(ch)?;

        for (_next_start, next_ch) in ch_indices.by_ref() {
            if let Some(next_node) = node.get_child(next_ch) {
                node = next_node;
            } else {
                break;
            }
        }

        if node.is_word_end {
            node.replacement.clone()
        } else {
            None
        }
    }

    fn skip_to_word_boundary<I>(&self, text: &str, start: usize, ch_indices: &mut I) -> usize
    where
        I: Iterator<Item = (usize, char)>,
    {
        let mut end = start;
        for (next_start, _) in ch_indices.by_ref() {
            if self.is_word_boundary(text, next_start) {
                end = next_start;
                break;
            }
        }
        end
    }

    fn is_word_boundary(&self, text: &str, index: usize) -> bool {
        if index >= text.len() {
            return true;
        }

        let ch = text.chars().nth(index).unwrap();
        !ch.is_alphabetic()
    }
}

impl Anonymizer for FlashTextAnonymizer {
    fn anonymize(
        &self,
        text: &str,
        items: Option<HashMap<String, String>>,
    ) -> Result<(String, HashMap<String, String>), OnyxError> {
        self.replace_keywords(text, items)
            .map_err(|err| OnyxError::AnonymizerError(format!("Failed to anonymize text: {}", err)))
    }
}
