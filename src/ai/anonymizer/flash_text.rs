use std::{
    collections::HashMap,
    fs::File,
    io::{self, BufRead},
    path::PathBuf,
};

use anyhow::Result;
use pluralizer::pluralize;

use super::base::Anonymizer;

#[derive(Default, Debug, Clone)]
pub struct TrieNode {
    children: HashMap<char, TrieNode>,
    is_word_end: bool,
    case_insensitive: bool,
}

impl TrieNode {
    fn new(case_insensitive: bool) -> Self {
        TrieNode {
            children: HashMap::new(),
            is_word_end: false,
            case_insensitive,
        }
    }

    fn get_child(&self, ch: char) -> Option<&TrieNode> {
        self.children.get(&self.norm_char(ch))
    }

    fn get_or_create_child(&mut self, ch: char) -> &mut TrieNode {
        self.children
            .entry(self.norm_char(ch))
            .or_insert_with(|| TrieNode::new(self.case_insensitive))
    }

    fn norm_char(&self, ch: char) -> char {
        if self.case_insensitive {
            ch.to_lowercase().next().unwrap()
        } else {
            ch
        }
    }
}

#[derive(Debug, Clone)]
pub struct FlashTextAnonymizer {
    root: TrieNode,
    replacement: String,
    pluralize: bool,
}

impl FlashTextAnonymizer {
    pub fn new(replacement: String, pluralize: bool, case_insensitive: bool) -> Self {
        FlashTextAnonymizer {
            root: TrieNode::new(case_insensitive),
            replacement,
            pluralize,
        }
    }

    pub fn add_keywords_file(&mut self, path: &PathBuf) -> Result<()> {
        let file = File::open(path)
            .map_err(|err| anyhow::anyhow!("Failed to open file: {:?}. Error:\n{}", path, err))?;
        io::BufReader::new(file)
            .lines()
            .try_for_each(|word| -> Result<()> {
                let word = word?;
                self.add_keyword(&word)?;
                if self.pluralize {
                    let singular_word = pluralize(&word, 1, false);
                    let plural_word = pluralize(&word, 2, false);
                    self.add_keyword(&singular_word)?;
                    self.add_keyword(&plural_word)?;
                }
                Ok(())
            })?;

        Ok(())
    }

    pub fn add_keyword(&mut self, word: &str) -> Result<()> {
        let mut node: &mut TrieNode = &mut self.root;

        for ch in word.chars() {
            node = node.get_or_create_child(ch);
        }
        node.is_word_end = true;
        Ok(())
    }

    pub fn replace_keywords(
        &self,
        text: &str,
        items: Option<HashMap<String, String>>,
    ) -> Result<(String, HashMap<String, String>)> {
        let mut internal_text = text.to_string();
        internal_text.push_str("  ");
        let mut result = String::new();
        let mut ch_indices = internal_text.char_indices();
        let mut start = 0;

        let mut items: HashMap<String, String> = match items {
            Some(it) => it,
            None => HashMap::new(),
        };
        let mut idx = 0;

        let base_replacement = self.replacement.clone();

        while let Some((match_start, ch)) = ch_indices.next() {
            if let Some(_word) = self.traverse_trie(ch, &mut ch_indices) {
                result.push_str(&internal_text[start..match_start]);
                let mut rep = base_replacement.to_string();
                rep.push_str(&idx.to_string());
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
                        idx += 1;
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
        let mut chars = vec![ch];

        for (_next_start, next_ch) in ch_indices.by_ref() {
            if let Some(next_node) = node.get_child(next_ch) {
                chars.push(next_ch);
                node = next_node;
            } else {
                break;
            }
        }

        if node.is_word_end {
            Some(chars.into_iter().collect())
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
    ) -> Result<(String, HashMap<String, String>)> {
        self.replace_keywords(text, items)
    }
}
