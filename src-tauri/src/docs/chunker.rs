//! Simple, dependency-free text chunker. Splits on paragraph boundaries when
//! possible so chunks stay coherent, falling back to a hard character cut
//! for very long paragraphs. Character-based (not byte-based) so it behaves
//! correctly on Chinese text, which has no whitespace word boundaries.

const CHUNK_SIZE_CHARS: usize = 800;
const CHUNK_OVERLAP_CHARS: usize = 150;

pub fn chunk_text(text: &str) -> Vec<String> {
    let paragraphs: Vec<&str> = text
        .split("\n\n")
        .map(|p| p.trim())
        .filter(|p| !p.is_empty())
        .collect();

    let mut chunks = Vec::new();
    let mut current = String::new();

    for para in paragraphs {
        if current.chars().count() + para.chars().count() > CHUNK_SIZE_CHARS && !current.is_empty()
        {
            chunks.push(current.clone());
            current = carry_over(&current);
        }

        if para.chars().count() > CHUNK_SIZE_CHARS {
            // Paragraph itself is too long - hard-split it.
            if !current.is_empty() {
                chunks.push(current.clone());
                current.clear();
            }
            for piece in hard_split(para) {
                chunks.push(piece);
            }
            continue;
        }

        if !current.is_empty() {
            current.push_str("\n\n");
        }
        current.push_str(para);
    }

    if !current.trim().is_empty() {
        chunks.push(current);
    }

    if chunks.is_empty() && !text.trim().is_empty() {
        chunks = hard_split(text.trim());
    }

    chunks
}

fn carry_over(prev_chunk: &str) -> String {
    let chars: Vec<char> = prev_chunk.chars().collect();
    if chars.len() <= CHUNK_OVERLAP_CHARS {
        return String::new();
    }
    chars[chars.len() - CHUNK_OVERLAP_CHARS..].iter().collect()
}

fn hard_split(text: &str) -> Vec<String> {
    let chars: Vec<char> = text.chars().collect();
    let mut pieces = Vec::new();
    let mut start = 0;
    while start < chars.len() {
        let end = (start + CHUNK_SIZE_CHARS).min(chars.len());
        pieces.push(chars[start..end].iter().collect());
        if end == chars.len() {
            break;
        }
        start = end.saturating_sub(CHUNK_OVERLAP_CHARS);
    }
    pieces
}
