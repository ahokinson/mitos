const MAX_LCS_CELLS: usize = 4_000_000;

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum Sign {
    Context,
    Added,
    Removed,
}

impl Sign {
    pub fn symbol(self) -> char {
        match self {
            Self::Context => ' ',
            Self::Added => '+',
            Self::Removed => '-',
        }
    }

    fn from_symbol(symbol: char) -> Option<Self> {
        match symbol {
            ' ' => Some(Self::Context),
            '+' => Some(Self::Added),
            '-' => Some(Self::Removed),
            _ => None,
        }
    }
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct DiffLine<'a> {
    pub sign: Sign,
    pub text: &'a str,
}

impl<'a> DiffLine<'a> {
    fn new(sign: Sign, text: &'a str) -> Self {
        Self { sign, text }
    }

    /// A unified-diff body line: its first character is the sign.
    pub fn parse(raw: &'a str) -> Option<Self> {
        let mut chars = raw.chars();
        let sign = Sign::from_symbol(chars.next()?)?;
        Some(Self::new(sign, chars.as_str()))
    }
}

pub fn split_lines(text: &str) -> Vec<&str> {
    if text.is_empty() {
        return Vec::new();
    }
    let mut lines: Vec<&str> = text.split('\n').collect();
    if lines.last() == Some(&"") {
        lines.pop();
    }
    lines
}

pub fn line_ops<'a>(old: &[&'a str], new: &[&'a str]) -> Vec<DiffLine<'a>> {
    let head = old.iter().zip(new).take_while(|(a, b)| a == b).count();
    let tail = old[head..]
        .iter()
        .rev()
        .zip(new[head..].iter().rev())
        .take_while(|(a, b)| a == b)
        .count();
    let old_mid = &old[head..old.len() - tail];
    let new_mid = &new[head..new.len() - tail];

    let mut ops: Vec<DiffLine<'a>> = context(&old[..head]).collect();
    if old_mid.len() * new_mid.len() > MAX_LCS_CELLS {
        ops.extend(
            old_mid
                .iter()
                .map(|&text| DiffLine::new(Sign::Removed, text)),
        );
        ops.extend(new_mid.iter().map(|&text| DiffLine::new(Sign::Added, text)));
    } else {
        ops.extend(lcs_ops(old_mid, new_mid));
    }
    ops.extend(context(&old[old.len() - tail..]));
    ops
}

fn context<'a>(lines: &[&'a str]) -> impl Iterator<Item = DiffLine<'a>> {
    lines.iter().map(|&text| DiffLine::new(Sign::Context, text))
}

fn lcs_ops<'a>(old: &[&'a str], new: &[&'a str]) -> Vec<DiffLine<'a>> {
    let width = new.len() + 1;
    let mut table = vec![0u32; (old.len() + 1) * width];
    for i in (0..old.len()).rev() {
        for j in (0..new.len()).rev() {
            table[i * width + j] = if old[i] == new[j] {
                table[(i + 1) * width + j + 1] + 1
            } else {
                table[(i + 1) * width + j].max(table[i * width + j + 1])
            };
        }
    }

    let mut ops = Vec::new();
    let (mut i, mut j) = (0, 0);
    while i < old.len() && j < new.len() {
        if old[i] == new[j] {
            ops.push(DiffLine::new(Sign::Context, old[i]));
            i += 1;
            j += 1;
        } else if table[(i + 1) * width + j] >= table[i * width + j + 1] {
            ops.push(DiffLine::new(Sign::Removed, old[i]));
            i += 1;
        } else {
            ops.push(DiffLine::new(Sign::Added, new[j]));
            j += 1;
        }
    }
    ops.extend(
        old[i..]
            .iter()
            .map(|&text| DiffLine::new(Sign::Removed, text)),
    );
    ops.extend(
        new[j..]
            .iter()
            .map(|&text| DiffLine::new(Sign::Added, text)),
    );
    ops
}
