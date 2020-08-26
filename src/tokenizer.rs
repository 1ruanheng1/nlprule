use lazy_static::lazy_static;
use regex::Regex;
use std::collections::HashSet;
use unicode_segmentation::UnicodeSegmentation;

mod disambiguate;
mod tag;

use disambiguate::Disambiguator;
use tag::Tagger;

lazy_static! {
    static ref DISAMBIGUATOR: Disambiguator = {
        Disambiguator::from_xml(format!(
            "data/disambiguation.{}.canonic.xml",
            std::env::var("RULE_LANG").unwrap()
        ))
    };
}

lazy_static! {
    static ref TAGGER: Tagger = Tagger::from_dumps(format!(
        "data/dumps/{}",
        std::env::var("RULE_LANG").unwrap()
    ))
    .unwrap();
}

// see https://stackoverflow.com/a/40296745
fn split<F>(text: &str, split_func: F) -> Vec<&str>
where
    F: Fn(char) -> bool,
{
    let mut result = Vec::new();
    let mut last = 0;
    for (index, matched) in text.match_indices(split_func) {
        if last != index {
            result.push(&text[last..index]);
        }
        result.push(matched);
        last = index + matched.len();
    }
    if last < text.len() {
        result.push(&text[last..]);
    }

    result
}

fn get_token_strs(text: &str) -> Vec<&str> {
    let mut tokens = Vec::new();

    lazy_static! {
        // see https://stackoverflow.com/a/3809435 and https://regexr.com/3e6m0
        static ref URL_REGEX: Regex = Regex::new(r"(http(s)?://.)?(www\.)?[-a-zA-Z0-9@:%._\+~#=]{2,256}\.[a-z]{2,6}\b([-a-zA-Z0-9@:%_\+.~#?&//=]*)").unwrap();
    }

    let mut prev = 0;
    let split_func = |c: char| c.is_whitespace() || r##"'’`´‘],.:!?/\()<=>„“”"+#…*"##.contains(c);

    for m in URL_REGEX.find_iter(text) {
        tokens.extend(split(&text[prev..m.start()], split_func));
        tokens.push(&text[m.start()..m.end()]);
        prev = m.end();
    }

    tokens.extend(split(&text[prev..text.len()], split_func));

    tokens
}

#[derive(Debug)]
pub struct Token<'a> {
    pub text: &'a str,
    pub lower: String,
    pub tags: Vec<(String, String)>,
    pub inflections: Vec<String>,
    pub lower_inflections: Vec<String>,
    pub postags: Vec<String>,
    pub char_span: (usize, usize),
    pub byte_span: (usize, usize),
    pub has_space_before: bool,
}

impl<'a> Token<'a> {
    fn sent_start() -> Token<'static> {
        Token {
            text: "",
            inflections: Vec::new(),
            tags: Vec::new(),
            lower_inflections: Vec::new(),
            lower: String::new(),
            postags: vec!["SENT_START".to_string()],
            char_span: (0, 0),
            byte_span: (0, 0),
            has_space_before: false,
        }
    }
}

pub fn tokenize<'a>(text: &'a str) -> Vec<Token<'a>> {
    let _sentence_indices = text
        .unicode_sentences()
        .map(|sentence| {
            let ptr = sentence.as_ptr() as usize;
            (ptr, ptr + sentence.len())
        })
        .fold((HashSet::new(), HashSet::new()), |mut a, x| {
            a.0.insert(x.0);
            a.1.insert(x.1);
            a
        });

    let mut current_char = 0;

    let mut tokens = vec![Token::sent_start()];

    tokens.extend(
        get_token_strs(text)
            .into_iter()
            .map(|x| {
                let char_start = current_char;
                let _ptr = x.as_ptr() as usize;
                current_char += x.chars().count();

                let byte_start = x.as_ptr() as usize - text.as_ptr() as usize;
                let trimmed = x.trim();

                let lower = trimmed.to_lowercase();

                Token {
                    text: trimmed,
                    tags: TAGGER.get_tags(&lower),
                    lower,
                    inflections: Vec::new(),
                    lower_inflections: Vec::new(),
                    postags: Vec::new(),
                    char_span: (char_start, current_char),
                    byte_span: (byte_start, byte_start + x.len()),
                    has_space_before: text[..byte_start].ends_with(char::is_whitespace),
                }
            })
            .filter(|token| !token.text.is_empty()),
    );

    let mut tokens = DISAMBIGUATOR.apply(tokens);

    // postprocessing, should probably be handled by a TokenBuilder
    tokens.iter_mut().for_each(|x| {
        x.inflections = x.tags.iter().map(|x| x.0.clone()).collect();
        x.inflections.push(x.text.to_string());

        x.lower_inflections = x.inflections.iter().map(|x| x.to_lowercase()).collect();
        x.postags = x.tags.iter().map(|x| x.1.clone()).collect();

        if x.postags.is_empty() {
            x.postags = vec!["UNKNOWN".to_string()];
        }
    });

    tokens
}
