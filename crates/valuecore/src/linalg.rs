//! Symmetric eigendecomposition via Jacobi iteration.
//! Input: n×n symmetric matrix (row-major flat Vec<f64>).
//! Output: (eigenvalues, eigenvectors) where eigenvectors[i] is the i-th eigenvector as a row.
//! Convergence: iterates until max off-diagonal element < 1e-12 or 100*n² sweeps.

/// Compute eigendecomposition of a symmetric matrix.
/// Returns (eigenvalues, eigenvectors) where eigenvectors[i] is the i-th eigenvector.
pub fn eigh(matrix: &[f64], n: usize) -> (Vec<f64>, Vec<Vec<f64>>) {
    assert_eq!(matrix.len(), n * n, "matrix must be n×n");
    if n == 0 {
        return (vec![], vec![]);
    }

    // Working copy of matrix
    let mut a: Vec<f64> = matrix.to_vec();
    // Eigenvector matrix — start as identity
    let mut v = vec![0.0f64; n * n];
    for i in 0..n {
        v[i * n + i] = 1.0;
    }

    let max_sweeps = 100 * n * n;
    let eps = 1e-12;

    for _ in 0..max_sweeps {
        // Find max off-diagonal element
        let mut max_val = 0.0f64;
        let mut p = 0;
        let mut q = 1;
        for i in 0..n {
            for j in (i + 1)..n {
                let x = a[i * n + j].abs();
                if x > max_val {
                    max_val = x;
                    p = i;
                    q = j;
                }
            }
        }
        if max_val < eps {
            break;
        }

        // Compute Jacobi rotation angle
        let app = a[p * n + p];
        let aqq = a[q * n + q];
        let apq = a[p * n + q];
        let theta = if (aqq - app).abs() < 1e-15 {
            std::f64::consts::FRAC_PI_4
        } else {
            0.5 * ((2.0 * apq) / (aqq - app)).atan()
        };
        let c = theta.cos();
        let s = theta.sin();

        // Apply rotation: A' = G^T A G
        // Update rows/cols p and q
        let mut new_a = a.clone();

        // Diagonal elements
        new_a[p * n + p] = c * c * app - 2.0 * s * c * apq + s * s * aqq;
        new_a[q * n + q] = s * s * app + 2.0 * s * c * apq + c * c * aqq;
        new_a[p * n + q] = 0.0;
        new_a[q * n + p] = 0.0;

        // Other rows/cols
        for r in 0..n {
            if r == p || r == q {
                continue;
            }
            let arp = a[r * n + p];
            let arq = a[r * n + q];
            new_a[r * n + p] = c * arp - s * arq;
            new_a[p * n + r] = new_a[r * n + p];
            new_a[r * n + q] = s * arp + c * arq;
            new_a[q * n + r] = new_a[r * n + q];
        }
        a = new_a;

        // Update eigenvector matrix: V' = V G
        for r in 0..n {
            let vrp = v[r * n + p];
            let vrq = v[r * n + q];
            v[r * n + p] = c * vrp - s * vrq;
            v[r * n + q] = s * vrp + c * vrq;
        }
    }

    // Extract eigenvalues and eigenvectors
    let eigenvalues: Vec<f64> = (0..n).map(|i| a[i * n + i]).collect();
    // v columns are eigenvectors; return as rows
    let eigenvectors: Vec<Vec<f64>> = (0..n)
        .map(|i| (0..n).map(|j| v[j * n + i]).collect())
        .collect();

    (eigenvalues, eigenvectors)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn approx_eq(a: f64, b: f64, tol: f64) -> bool {
        (a - b).abs() < tol
    }

    #[test]
    fn identity_2x2() {
        let m = vec![1.0, 0.0, 0.0, 1.0];
        let (vals, vecs) = eigh(&m, 2);
        assert_eq!(vals.len(), 2);
        assert_eq!(vecs.len(), 2);
        // Both eigenvalues must be 1.0
        let mut sorted = vals.clone();
        sorted.sort_by(|a, b| a.partial_cmp(b).unwrap());
        assert!(approx_eq(sorted[0], 1.0, 1e-10));
        assert!(approx_eq(sorted[1], 1.0, 1e-10));
    }

    #[test]
    fn diagonal_2x2() {
        // Diagonal matrix: eigenvalues are the diagonal entries
        let m = vec![3.0, 0.0, 0.0, 7.0];
        let (vals, _vecs) = eigh(&m, 2);
        let mut sorted = vals.clone();
        sorted.sort_by(|a, b| a.partial_cmp(b).unwrap());
        assert!(approx_eq(sorted[0], 3.0, 1e-10), "got {:?}", sorted);
        assert!(approx_eq(sorted[1], 7.0, 1e-10), "got {:?}", sorted);
    }

    #[test]
    fn known_2x2() {
        // [[2, 1], [1, 2]] => eigenvalues 1 and 3
        let m = vec![2.0, 1.0, 1.0, 2.0];
        let (vals, vecs) = eigh(&m, 2);
        let mut pairs: Vec<(f64, Vec<f64>)> = vals.into_iter().zip(vecs.into_iter()).collect();
        pairs.sort_by(|a, b| a.0.partial_cmp(&b.0).unwrap());
        assert!(approx_eq(pairs[0].0, 1.0, 1e-10), "got {}", pairs[0].0);
        assert!(approx_eq(pairs[1].0, 3.0, 1e-10), "got {}", pairs[1].0);
        // Verify A v = lambda v for each eigenpair
        for (lam, vec) in &pairs {
            let mv0 = 2.0 * vec[0] + 1.0 * vec[1];
            let mv1 = 1.0 * vec[0] + 2.0 * vec[1];
            assert!(approx_eq(mv0, lam * vec[0], 1e-9));
            assert!(approx_eq(mv1, lam * vec[1], 1e-9));
        }
    }

    #[test]
    fn known_3x3() {
        // [[4,2,0],[2,3,1],[0,1,2]]
        let m = vec![4.0,2.0,0.0, 2.0,3.0,1.0, 0.0,1.0,2.0];
        let (vals, vecs) = eigh(&m, 3);
        // Verify A v = lambda v for each eigenpair
        for (lam, vec) in vals.iter().zip(vecs.iter()) {
            let mv0 = 4.0*vec[0] + 2.0*vec[1] + 0.0*vec[2];
            let mv1 = 2.0*vec[0] + 3.0*vec[1] + 1.0*vec[2];
            let mv2 = 0.0*vec[0] + 1.0*vec[1] + 2.0*vec[2];
            assert!(approx_eq(mv0, lam * vec[0], 1e-8), "eigenpair check failed");
            assert!(approx_eq(mv1, lam * vec[1], 1e-8), "eigenpair check failed");
            assert!(approx_eq(mv2, lam * vec[2], 1e-8), "eigenpair check failed");
        }
    }

    #[test]
    fn empty() {
        let (vals, vecs) = eigh(&[], 0);
        assert_eq!(vals.len(), 0);
        assert_eq!(vecs.len(), 0);
    }

    #[test]
    fn periodic_table_regression() {
        // 5x5 matrix from collapse_periodic_table — check trace = sum of eigenvalues
        let m = vec![
            1.0, 0.1, 0.1, 0.1, 0.1,
            0.1, 2.0, 0.1, 0.1, 0.1,
            0.1, 0.1, 3.0, 0.1, 0.1,
            0.1, 0.1, 0.1, 4.0, 0.1,
            0.1, 0.1, 0.1, 0.1, 5.0,
        ];
        let (vals, _) = eigh(&m, 5);
        let trace_m: f64 = 1.0 + 2.0 + 3.0 + 4.0 + 5.0;
        let trace_v: f64 = vals.iter().sum();
        assert!(approx_eq(trace_m, trace_v, 1e-8), "trace mismatch: {} vs {}", trace_m, trace_v);
    }
}
