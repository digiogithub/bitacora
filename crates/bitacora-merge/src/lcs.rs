//! Order-preserving longest common subsequence, shared by the block matcher and the line diff3.

/// Longest common subsequence of `a` and `b` as index pairs `(i, j)` in increasing order.
#[must_use]
pub fn lcs_pairs<T: PartialEq>(a: &[T], b: &[T]) -> Vec<(usize, usize)> {
    let (n, m) = (a.len(), b.len());
    let w = m + 1;
    let mut dp = vec![0u32; (n + 1) * w];
    for i in (0..n).rev() {
        for j in (0..m).rev() {
            dp[i * w + j] = if a[i] == b[j] {
                dp[(i + 1) * w + j + 1] + 1
            } else {
                dp[(i + 1) * w + j].max(dp[i * w + j + 1])
            };
        }
    }
    let (mut i, mut j) = (0, 0);
    let mut out = Vec::new();
    while i < n && j < m {
        if a[i] == b[j] {
            out.push((i, j));
            i += 1;
            j += 1;
        } else if dp[(i + 1) * w + j] >= dp[i * w + j + 1] {
            i += 1;
        } else {
            j += 1;
        }
    }
    out
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn finds_ordered_common_subsequence() {
        assert_eq!(lcs_pairs(&[1, 2, 3, 4], &[2, 4, 5]), [(1, 0), (3, 1)]);
        assert!(lcs_pairs::<u8>(&[], &[1]).is_empty());
    }
}
