//! Content expression parser and DFA for runtime schema compilation.
//!
//! Parses content expression strings like `"block+"`, `"inline*"`,
//! `"paragraph block*"` into a deterministic finite automaton that can
//! match sequences of node types.

use indexmap::IndexMap;
use serde::{Deserialize, Serialize};
use std::collections::HashMap;

/// A compiled content expression DFA.
///
/// Each state in the DFA is an index into a vector of `ContentState` entries.
/// State 0 is the start state. A state is a valid end state if
/// `ContentExpr.states[i].valid_end` is true.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ContentExpr {
    /// The DFA states
    pub states: Vec<ContentState>,
}

/// A single state in the content expression DFA.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ContentState {
    /// Transitions: maps a node type name to the next state index
    pub edges: IndexMap<String, usize>,
    /// Whether this state represents a valid end of the content
    pub valid_end: bool,
}

/// Errors during content expression parsing.
#[derive(Debug, Clone)]
pub enum ContentExprError {
    /// Unexpected character in the expression
    UnexpectedChar(char),
    /// Unknown group or node type reference
    UnknownRef(String),
    /// Mismatched parentheses
    MismatchedParens,
    /// Empty expression in a context that requires non-empty
    EmptyExpr,
    /// Invalid operator usage
    InvalidOperator,
}

impl std::fmt::Display for ContentExprError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::UnexpectedChar(c) => write!(f, "Unexpected character: {}", c),
            Self::UnknownRef(r) => write!(f, "Unknown reference: {}", r),
            Self::MismatchedParens => write!(f, "Mismatched parentheses"),
            Self::EmptyExpr => write!(f, "Empty expression"),
            Self::InvalidOperator => write!(f, "Invalid operator"),
        }
    }
}

impl std::error::Error for ContentExprError {}

/// An atom in a content expression: a node type name or group name.
#[derive(Debug, Clone)]
enum ExprAtom {
    /// A specific node type name
    Name(String),
    /// A group of node types
    Group(String),
    /// Any inline node
    Inline,
    /// Any block node
    Block,
    /// A parenthesized sub-expression (alternatives of sequences)
    Nested(Vec<Vec<ExprElement>>),
}

/// A content expression element with a quantifier.
#[derive(Debug, Clone)]
struct ExprElement {
    /// The atom being matched
    atom: ExprAtom,
    /// The quantifier: `?`, `*`, `+`, or none (exactly once)
    quantifier: Quantifier,
}

/// Quantifier for a content expression element.
#[derive(Debug, Clone, Copy, PartialEq)]
enum Quantifier {
    /// Exactly once
    Once,
    /// Zero or one
    Optional,
    /// Zero or more
    Star,
    /// One or more
    Plus,
    /// A counted range: min required, max optional (None = unbounded)
    Range { min: usize, max: Option<usize> },
}

/// Token in the content expression lexer.
#[derive(Debug, Clone)]
enum Token {
    /// A name (node type or group)
    Name(String),
    /// `+`, `*`, `?` quantifiers
    Plus,
    Star,
    Question,
    /// `|`
    Pipe,
    /// `(`
    OpenParen,
    /// `)`
    CloseParen,
    /// `{`
    OpenBrace,
    /// `}`
    CloseBrace,
    /// `,`
    Comma,
    /// A number literal
    Number(usize),
    /// End of input
    Eof,
}

struct Lexer {
    input: Vec<char>,
    pos: usize,
}

impl Lexer {
    fn new(input: &str) -> Self {
        Lexer {
            input: input.chars().collect(),
            pos: 0,
        }
    }

    fn next_token(&mut self) -> Result<Token, ContentExprError> {
        while self.pos < self.input.len() && self.input[self.pos].is_whitespace() {
            self.pos += 1;
        }
        if self.pos >= self.input.len() {
            return Ok(Token::Eof);
        }
        let c = self.input[self.pos];
        match c {
            '+' => {
                self.pos += 1;
                Ok(Token::Plus)
            }
            '*' => {
                self.pos += 1;
                Ok(Token::Star)
            }
            '?' => {
                self.pos += 1;
                Ok(Token::Question)
            }
            '|' => {
                self.pos += 1;
                Ok(Token::Pipe)
            }
            '(' => {
                self.pos += 1;
                Ok(Token::OpenParen)
            }
            ')' => {
                self.pos += 1;
                Ok(Token::CloseParen)
            }
            '{' => {
                self.pos += 1;
                Ok(Token::OpenBrace)
            }
            '}' => {
                self.pos += 1;
                Ok(Token::CloseBrace)
            }
            ',' => {
                self.pos += 1;
                Ok(Token::Comma)
            }
            _ if c.is_ascii_digit() => {
                let start = self.pos;
                while self.pos < self.input.len() && self.input[self.pos].is_ascii_digit() {
                    self.pos += 1;
                }
                let num_str: String = self.input[start..self.pos].iter().collect();
                let num = num_str
                    .parse()
                    .map_err(|_| ContentExprError::InvalidOperator)?;
                Ok(Token::Number(num))
            }
            _ if c.is_alphanumeric() || c == '_' || c == '-' => {
                let start = self.pos;
                while self.pos < self.input.len()
                    && (self.input[self.pos].is_alphanumeric()
                        || self.input[self.pos] == '_'
                        || self.input[self.pos] == '-')
                {
                    self.pos += 1;
                }
                let name: String = self.input[start..self.pos].iter().collect();
                Ok(Token::Name(name))
            }
            _ => Err(ContentExprError::UnexpectedChar(c)),
        }
    }
}

/// Parse a content expression string into a compiled `ContentExpr` DFA.
///
/// The `groups` map should map group names to the set of node type names in
/// each group.
pub fn parse_content_expr(
    input: &str,
    groups: &HashMap<String, Vec<String>>,
) -> Result<ContentExpr, ContentExprError> {
    let input = input.trim();
    if input.is_empty() {
        // Empty content: single accepting state with no edges
        return Ok(ContentExpr {
            states: vec![ContentState {
                edges: IndexMap::new(),
                valid_end: true,
            }],
        });
    }

    let mut lexer = Lexer::new(input);
    let alternatives = parse_expr(&mut lexer, groups)?;
    match lexer.next_token()? {
        Token::Eof => {}
        Token::CloseParen => return Err(ContentExprError::MismatchedParens),
        _ => return Err(ContentExprError::InvalidOperator),
    }

    // Build NFA then convert to DFA
    let nfa = build_nfa(&alternatives, groups)?;
    let dfa = nfa_to_dfa(&nfa);
    Ok(dfa)
}

fn parse_expr(
    lexer: &mut Lexer,
    groups: &HashMap<String, Vec<String>>,
) -> Result<Vec<Vec<ExprElement>>, ContentExprError> {
    let mut alternatives = Vec::new();
    alternatives.push(parse_nonempty_sequence(lexer, groups)?);

    loop {
        match lexer.next_token()? {
            Token::Pipe => {
                alternatives.push(parse_nonempty_sequence(lexer, groups)?);
            }
            Token::Eof => break,
            Token::CloseParen => {
                // Put it back (caller handles closing paren)
                lexer.pos -= 1;
                break;
            }
            _ => return Err(ContentExprError::InvalidOperator),
        }
    }

    Ok(alternatives)
}

fn parse_nonempty_sequence(
    lexer: &mut Lexer,
    groups: &HashMap<String, Vec<String>>,
) -> Result<Vec<ExprElement>, ContentExprError> {
    let sequence = parse_sequence(lexer, groups)?;
    if sequence.is_empty() {
        return Err(ContentExprError::EmptyExpr);
    }
    Ok(sequence)
}

fn parse_sequence(
    lexer: &mut Lexer,
    groups: &HashMap<String, Vec<String>>,
) -> Result<Vec<ExprElement>, ContentExprError> {
    let mut elements = Vec::new();
    loop {
        let saved = lexer.pos;
        match lexer.next_token()? {
            Token::Name(name) => {
                let atom = match name.as_str() {
                    "inline" => ExprAtom::Inline,
                    "block" => ExprAtom::Block,
                    _ => {
                        if name.chars().next().is_some_and(|c| c.is_uppercase())
                            || groups.contains_key(&name)
                        {
                            ExprAtom::Group(name)
                        } else {
                            ExprAtom::Name(name)
                        }
                    }
                };
                let quantifier = parse_quantifier(lexer)?;
                elements.push(ExprElement { atom, quantifier });
            }
            Token::OpenParen => {
                let inner = parse_expr(lexer, groups)?;
                match lexer.next_token()? {
                    Token::CloseParen => {}
                    _ => return Err(ContentExprError::MismatchedParens),
                }
                let quantifier = parse_quantifier(lexer)?;
                if quantifier == Quantifier::Once && inner.len() == 1 {
                    // Flatten single-alternative parenthesized sequences
                    for elem in inner.into_iter().next().unwrap() {
                        elements.push(elem);
                    }
                } else {
                    elements.push(ExprElement {
                        atom: ExprAtom::Nested(inner),
                        quantifier,
                    });
                }
            }
            Token::Eof | Token::Pipe | Token::CloseParen => {
                // Restore position so the caller can see this token
                lexer.pos = saved;
                break;
            }
            _ => return Err(ContentExprError::UnexpectedChar('?')),
        }
    }
    Ok(elements)
}

fn parse_quantifier(lexer: &mut Lexer) -> Result<Quantifier, ContentExprError> {
    // Peek at the next token without consuming
    let saved = lexer.pos;
    match lexer.next_token()? {
        Token::Plus => Ok(Quantifier::Plus),
        Token::Star => Ok(Quantifier::Star),
        Token::Question => Ok(Quantifier::Optional),
        Token::OpenBrace => {
            let min = match lexer.next_token()? {
                Token::Number(n) => n,
                _ => return Err(ContentExprError::InvalidOperator),
            };
            match lexer.next_token()? {
                Token::CloseBrace => Ok(Quantifier::Range {
                    min,
                    max: Some(min),
                }),
                Token::Comma => match lexer.next_token()? {
                    Token::CloseBrace => Ok(Quantifier::Range { min, max: None }),
                    Token::Number(max) => match lexer.next_token()? {
                        Token::CloseBrace => Ok(Quantifier::Range {
                            min,
                            max: Some(max),
                        }),
                        _ => Err(ContentExprError::InvalidOperator),
                    },
                    _ => Err(ContentExprError::InvalidOperator),
                },
                _ => Err(ContentExprError::InvalidOperator),
            }
        }
        _ => {
            lexer.pos = saved;
            Ok(Quantifier::Once)
        }
    }
}

/// A simple NFA state.
#[derive(Debug, Clone)]
struct NfaState {
    /// Epsilon transitions to other NFA states
    epsilon: Vec<usize>,
    /// Transitions on a node type name to other NFA states
    edges: Vec<(String, usize)>,
    /// Whether this is an accepting state
    valid_end: bool,
}

/// Copy a nested NFA into the parent NFA, returning (start_state, merge_state).
fn copy_nested_nfa(
    states: &mut Vec<NfaState>,
    nested_alts: &[Vec<ExprElement>],
    groups: &HashMap<String, Vec<String>>,
) -> Result<(usize, usize), ContentExprError> {
    let nested_nfa = build_nfa(nested_alts, groups)?;
    let offset = states.len();
    for state in &nested_nfa {
        let mut state = state.clone();
        for e in &mut state.epsilon {
            *e += offset;
        }
        for (_, e) in &mut state.edges {
            *e += offset;
        }
        state.valid_end = false;
        states.push(state);
    }
    let start = offset;
    let accepts: Vec<usize> = nested_nfa
        .iter()
        .enumerate()
        .filter(|(_, s)| s.valid_end)
        .map(|(i, _)| i + offset)
        .collect();
    // Create a merge state for all accept states
    let merge = states.len();
    states.push(NfaState {
        epsilon: Vec::new(),
        edges: Vec::new(),
        valid_end: false,
    });
    for &acc in &accepts {
        states[acc].epsilon.push(merge);
    }
    Ok((start, merge))
}

fn build_nfa(
    alternatives: &[Vec<ExprElement>],
    groups: &HashMap<String, Vec<String>>,
) -> Result<Vec<NfaState>, ContentExprError> {
    let mut states = Vec::new();

    // Start state
    states.push(NfaState {
        epsilon: Vec::new(),
        edges: Vec::new(),
        valid_end: false,
    });

    // For each alternative, build a path through the NFA
    let mut accept_states = Vec::new();

    for alt in alternatives {
        let mut current = 0; // start state

        for elem in alt {
            match &elem.atom {
                ExprAtom::Nested(nested_alts) => {
                    match elem.quantifier {
                        Quantifier::Once => {
                            let (start, merge) = copy_nested_nfa(&mut states, nested_alts, groups)?;
                            states[current].epsilon.push(start);
                            current = merge;
                        }
                        Quantifier::Optional => {
                            let (start, merge) = copy_nested_nfa(&mut states, nested_alts, groups)?;
                            let next = states.len();
                            states.push(NfaState {
                                epsilon: Vec::new(),
                                edges: Vec::new(),
                                valid_end: false,
                            });
                            states[current].epsilon.push(next); // skip
                            states[current].epsilon.push(start); // match
                            states[merge].epsilon.push(next); // done
                            current = next;
                        }
                        Quantifier::Star => {
                            let (start, merge) = copy_nested_nfa(&mut states, nested_alts, groups)?;
                            let next = states.len();
                            states.push(NfaState {
                                epsilon: Vec::new(),
                                edges: Vec::new(),
                                valid_end: false,
                            });
                            states[current].epsilon.push(next); // skip
                            states[current].epsilon.push(start); // match
                            states[merge].epsilon.push(start); // loop back
                            states[merge].epsilon.push(next); // done
                            current = next;
                        }
                        Quantifier::Plus => {
                            let (start, merge) = copy_nested_nfa(&mut states, nested_alts, groups)?;
                            let next = states.len();
                            states.push(NfaState {
                                epsilon: Vec::new(),
                                edges: Vec::new(),
                                valid_end: false,
                            });
                            states[current].epsilon.push(start); // must match once
                            states[merge].epsilon.push(start); // loop back
                            states[merge].epsilon.push(next); // done
                            current = next;
                        }
                        Quantifier::Range { min, max } => {
                            let mut prev = current;
                            for _ in 0..min {
                                let (start, merge) =
                                    copy_nested_nfa(&mut states, nested_alts, groups)?;
                                states[prev].epsilon.push(start);
                                prev = merge;
                            }
                            match max {
                                Some(max) if max > min => {
                                    for _ in min..max {
                                        let (start, merge) =
                                            copy_nested_nfa(&mut states, nested_alts, groups)?;
                                        let next = states.len();
                                        states.push(NfaState {
                                            epsilon: Vec::new(),
                                            edges: Vec::new(),
                                            valid_end: false,
                                        });
                                        states[prev].epsilon.push(next); // skip
                                        states[prev].epsilon.push(start); // match
                                        states[merge].epsilon.push(next); // done
                                        prev = next;
                                    }
                                    current = prev;
                                }
                                None => {
                                    let (start, merge) =
                                        copy_nested_nfa(&mut states, nested_alts, groups)?;
                                    let next = states.len();
                                    states.push(NfaState {
                                        epsilon: Vec::new(),
                                        edges: Vec::new(),
                                        valid_end: false,
                                    });
                                    states[prev].epsilon.push(next); // skip
                                    states[prev].epsilon.push(start); // match
                                    states[merge].epsilon.push(start); // loop back
                                    states[merge].epsilon.push(next); // done
                                    current = next;
                                }
                                _ => {
                                    current = prev;
                                }
                            }
                        }
                    }
                }
                _ => {
                    let node_names = resolve_atom(&elem.atom, groups)?;

                    match elem.quantifier {
                        Quantifier::Once => {
                            let next = states.len();
                            states.push(NfaState {
                                epsilon: Vec::new(),
                                edges: Vec::new(),
                                valid_end: false,
                            });
                            for name in &node_names {
                                states[current].edges.push((name.clone(), next));
                            }
                            current = next;
                        }
                        Quantifier::Optional => {
                            let next = states.len();
                            states.push(NfaState {
                                epsilon: Vec::new(),
                                edges: Vec::new(),
                                valid_end: false,
                            });
                            // Epsilon transition (skip)
                            states[current].epsilon.push(next);
                            // Or match and advance
                            for name in &node_names {
                                states[current].edges.push((name.clone(), next));
                            }
                            current = next;
                        }
                        Quantifier::Star => {
                            let next = states.len();
                            states.push(NfaState {
                                epsilon: Vec::new(),
                                edges: Vec::new(),
                                valid_end: false,
                            });
                            // Epsilon transition (skip)
                            states[current].epsilon.push(next);
                            // Or match and loop back
                            for name in &node_names {
                                states[current].edges.push((name.clone(), current));
                            }
                            current = next;
                        }
                        Quantifier::Plus => {
                            // First, match at least one
                            let mid = states.len();
                            states.push(NfaState {
                                epsilon: Vec::new(),
                                edges: Vec::new(),
                                valid_end: false,
                            });
                            for name in &node_names {
                                states[current].edges.push((name.clone(), mid));
                            }
                            let next = states.len();
                            states.push(NfaState {
                                epsilon: Vec::new(),
                                edges: Vec::new(),
                                valid_end: false,
                            });
                            // From mid, can loop back or advance
                            states[mid].epsilon.push(next);
                            for name in &node_names {
                                states[mid].edges.push((name.clone(), mid));
                            }
                            current = next;
                        }
                        Quantifier::Range { min, max } => {
                            // Emit `min` required copies
                            for _ in 0..min {
                                let next = states.len();
                                states.push(NfaState {
                                    epsilon: Vec::new(),
                                    edges: Vec::new(),
                                    valid_end: false,
                                });
                                for name in &node_names {
                                    states[current].edges.push((name.clone(), next));
                                }
                                current = next;
                            }
                            match max {
                                Some(max) if max > min => {
                                    // Emit optional copies up to max
                                    for _ in min..max {
                                        let next = states.len();
                                        states.push(NfaState {
                                            epsilon: Vec::new(),
                                            edges: Vec::new(),
                                            valid_end: false,
                                        });
                                        states[current].epsilon.push(next); // skip
                                        for name in &node_names {
                                            states[current].edges.push((name.clone(), next));
                                        }
                                        current = next;
                                    }
                                }
                                None => {
                                    // Unbounded: star after the required copies
                                    let next = states.len();
                                    states.push(NfaState {
                                        epsilon: Vec::new(),
                                        edges: Vec::new(),
                                        valid_end: false,
                                    });
                                    states[current].epsilon.push(next); // skip
                                    for name in &node_names {
                                        states[current].edges.push((name.clone(), current));
                                        // loop
                                    }
                                    current = next;
                                }
                                _ => {} // max == min, no optional copies
                            }
                        }
                    }
                }
            }
        }

        accept_states.push(current);
    }

    // Mark accept states
    for &s in &accept_states {
        states[s].valid_end = true;
    }

    Ok(states)
}

fn resolve_atom(
    atom: &ExprAtom,
    groups: &HashMap<String, Vec<String>>,
) -> Result<Vec<String>, ContentExprError> {
    match atom {
        ExprAtom::Name(name) => Ok(vec![name.clone()]),
        ExprAtom::Group(name) => groups
            .get(name)
            .cloned()
            .ok_or_else(|| ContentExprError::UnknownRef(name.clone())),
        ExprAtom::Nested(_) => {
            panic!(
                "Nested expressions should be handled directly in build_nfa, not resolved to names"
            )
        }
        ExprAtom::Inline => {
            // Collect all inline types from groups
            let mut names = Vec::new();
            if let Some(inline) = groups.get("inline") {
                names.extend(inline.iter().cloned());
            }
            if names.is_empty() {
                // Fallback: treat as a group name
                return Err(ContentExprError::UnknownRef("inline".to_string()));
            }
            Ok(names)
        }
        ExprAtom::Block => {
            let mut names = Vec::new();
            if let Some(block) = groups.get("block") {
                names.extend(block.iter().cloned());
            }
            if names.is_empty() {
                return Err(ContentExprError::UnknownRef("block".to_string()));
            }
            Ok(names)
        }
    }
}

/// Convert an NFA to a DFA using subset construction.
fn nfa_to_dfa(nfa: &[NfaState]) -> ContentExpr {
    // Compute epsilon closure for each state
    let n = nfa.len();
    let mut eps_closure = vec![Vec::new(); n];
    for (i, closure) in eps_closure.iter_mut().enumerate() {
        let mut visited = std::collections::HashSet::new();
        let mut stack = vec![i];
        while let Some(s) = stack.pop() {
            if visited.insert(s) {
                for &next in &nfa[s].epsilon {
                    stack.push(next);
                }
            }
        }
        *closure = visited.into_iter().collect();
        closure.sort();
    }

    // Start state of DFA = epsilon closure of NFA state 0
    let start = eps_closure[0].clone();

    let mut dfa_states = Vec::new();
    let mut state_map: HashMap<Vec<usize>, usize> = HashMap::new();
    state_map.insert(start.clone(), 0);

    let start_valid = start.iter().any(|&s| nfa[s].valid_end);
    dfa_states.push(ContentState {
        edges: IndexMap::new(),
        valid_end: start_valid,
    });

    let mut queue = vec![start];
    let mut queue_idx = 0;

    while queue_idx < queue.len() {
        let current_set = queue[queue_idx].clone();
        let current_idx = queue_idx;
        queue_idx += 1;

        // Collect all possible transitions from this set
        // Iterate in reverse order to match JS edge ordering (descending NFA state order)
        let mut transitions: IndexMap<String, Vec<usize>> = IndexMap::new();
        for &state in current_set.iter().rev() {
            for (name, target) in &nfa[state].edges {
                transitions
                    .entry(name.clone())
                    .or_default()
                    .extend(eps_closure[*target].iter());
            }
        }

        for (name, mut targets) in transitions {
            targets.sort();
            targets.dedup();

            let next_idx = if let Some(&idx) = state_map.get(&targets) {
                idx
            } else {
                let idx = dfa_states.len();
                let valid = targets.iter().any(|&s| nfa[s].valid_end);
                dfa_states.push(ContentState {
                    edges: IndexMap::new(),
                    valid_end: valid,
                });
                state_map.insert(targets.clone(), idx);
                queue.push(targets);
                idx
            };

            dfa_states[current_idx].edges.insert(name, next_idx);
        }
    }

    ContentExpr { states: dfa_states }
}

impl ContentExpr {
    /// Create an empty content expression (matches nothing).
    pub fn empty() -> Self {
        ContentExpr {
            states: vec![ContentState {
                edges: IndexMap::new(),
                valid_end: true,
            }],
        }
    }

    /// Try to match a node type name at the current state, returning
    /// the next state index if successful.
    pub fn match_type(&self, state: usize, type_name: &str) -> Option<usize> {
        self.states.get(state)?.edges.get(type_name).copied()
    }

    /// Whether the given state is a valid end state.
    pub fn valid_end(&self, state: usize) -> bool {
        self.states.get(state).is_some_and(|s| s.valid_end)
    }

    /// Get the number of outgoing edges from a state.
    pub fn edge_count(&self, state: usize) -> usize {
        self.states.get(state).map_or(0, |s| s.edges.len())
    }

    /// Get the nth outgoing edge from a state as (type_name, next_state).
    pub fn edge(&self, state: usize, n: usize) -> Option<(&str, usize)> {
        let s = self.states.get(state)?;
        s.edges.iter().nth(n).map(|(k, v)| (k.as_str(), *v))
    }

    /// Match a sequence of node type names, returning the final state.
    pub fn match_fragment(&self, type_names: &[&str]) -> Option<usize> {
        let mut state = 0;
        for name in type_names {
            state = self.match_type(state, name)?;
        }
        Some(state)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_parse_empty() {
        let expr = parse_content_expr("", &HashMap::new()).unwrap();
        assert!(expr.valid_end(0));
        assert_eq!(expr.states.len(), 1);
    }

    #[test]
    fn test_empty_and_none_are_regular_node_names() {
        for name in ["empty", "none"] {
            let expr = parse_content_expr(name, &HashMap::new()).unwrap();
            assert!(!expr.valid_end(0));
            assert_eq!(expr.match_type(0, name), Some(1));
            assert!(expr.valid_end(1));
        }
    }

    #[test]
    fn test_parse_rejects_unconsumed_close_parens() {
        for input in ["paragraph)", "paragraph heading)", "(paragraph))"] {
            assert!(
                matches!(
                    parse_content_expr(input, &HashMap::new()),
                    Err(ContentExprError::MismatchedParens)
                ),
                "expected {:?} to reject the unmatched trailing parenthesis",
                input
            );
        }
    }

    #[test]
    fn test_parse_rejects_empty_alternatives() {
        for input in [
            "| paragraph",
            "paragraph |",
            "paragraph || heading",
            "()",
            "(paragraph |)",
        ] {
            assert!(
                matches!(
                    parse_content_expr(input, &HashMap::new()),
                    Err(ContentExprError::EmptyExpr)
                ),
                "expected {:?} to reject empty alternatives",
                input
            );
        }
    }

    #[test]
    fn test_parse_single_type() {
        let expr = parse_content_expr("paragraph", &HashMap::new()).unwrap();
        assert_eq!(expr.states.len(), 2);
        assert!(!expr.valid_end(0));
        assert!(expr.valid_end(1));
        assert_eq!(expr.match_type(0, "paragraph"), Some(1));
        assert_eq!(expr.match_type(0, "heading"), None);
    }

    #[test]
    fn test_parse_plus() {
        let mut groups = HashMap::new();
        groups.insert(
            "block".to_string(),
            vec!["paragraph".to_string(), "heading".to_string()],
        );
        let expr = parse_content_expr("block+", &groups).unwrap();
        assert!(!expr.valid_end(0));
        assert!(expr.match_type(0, "paragraph").is_some());
        assert!(expr.match_type(0, "heading").is_some());
        // Can match multiple
        let s1 = expr.match_type(0, "paragraph").unwrap();
        assert!(expr.valid_end(s1));
        let s2 = expr.match_type(s1, "heading").unwrap();
        assert!(expr.valid_end(s2));
    }

    #[test]
    fn test_parse_star() {
        let expr = parse_content_expr("paragraph*", &HashMap::new()).unwrap();
        assert!(expr.valid_end(0)); // star means zero is ok
        assert!(expr.match_type(0, "paragraph").is_some());
    }

    #[test]
    fn test_parse_sequence() {
        let expr = parse_content_expr("paragraph heading", &HashMap::new()).unwrap();
        assert!(!expr.valid_end(0));
        let s1 = expr.match_type(0, "paragraph").unwrap();
        assert!(!expr.valid_end(s1));
        let s2 = expr.match_type(s1, "heading").unwrap();
        assert!(expr.valid_end(s2));
    }

    #[test]
    fn test_parse_alternative() {
        let expr = parse_content_expr("paragraph | heading", &HashMap::new()).unwrap();
        assert!(!expr.valid_end(0));
        let s1 = expr.match_type(0, "paragraph").unwrap();
        assert!(expr.valid_end(s1));
        let s2 = expr.match_type(0, "heading").unwrap();
        assert!(expr.valid_end(s2));
    }
}
