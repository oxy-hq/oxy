use ndarray::{Array1, Array2, Axis};

pub(super) struct MathUtils;

impl MathUtils {
    pub(super) fn calculate_centroid(points: &[Vec<f32>], n_dims: usize) -> Vec<f32> {
        if points.is_empty() {
            return vec![0.0; n_dims];
        }
        
        let flat: Vec<f32> = points.iter().flatten().cloned().collect();
        let matrix = Array2::from_shape_vec((points.len(), n_dims), flat).unwrap();
        let centroid = Self::calculate_centroid_from_matrix(&matrix);
        centroid.to_vec()
    }

    pub(super) fn find_min_distance(point: &[f32], points: &[Vec<f32>]) -> Result<Option<f32>, String> {
        if points.is_empty() {
            return Ok(None);
        }
        
        let point_dims = point.len();
        let mut flat_points = Vec::new();
        for other_point in points {
            if !other_point.is_empty() {
                if other_point.len() != point_dims {
                    return Err(format!(
                        "Dimension mismatch: expected {} dimensions, found {} dimensions", 
                        point_dims, 
                        other_point.len()
                    ));
                }
                flat_points.extend_from_slice(other_point);
            }
        }
        
        if flat_points.is_empty() {
            return Ok(None);
        }
        
        let points_matrix = Array2::from_shape_vec((points.len(), point_dims), flat_points)
            .map_err(|e| format!("Failed to create matrix from points: {}", e))?;
        let point_array = Array1::from_vec(point.to_vec());
        Self::find_min_distance_from_matrix(&point_array, &points_matrix)
    }

    fn calculate_centroid_from_matrix(points_matrix: &Array2<f32>) -> Array1<f32> {
        if points_matrix.is_empty() {
            return Array1::zeros(points_matrix.ncols());
        }
        
        let mut centroid = points_matrix.mean_axis(Axis(0)).unwrap();
        let norm = Self::l2_norm(&centroid);
        if norm > 0.0 {
            centroid /= norm;
        }
        
        centroid
    }

    fn find_min_distance_from_matrix(
        point: &Array1<f32>,
        points_matrix: &Array2<f32>,
    ) -> Result<Option<f32>, String> {
        if points_matrix.is_empty() {
            return Ok(None);
        }
        
        let point_dims = point.len();
        let matrix_dims = points_matrix.ncols();
        
        if matrix_dims != point_dims {
            return Err(format!(
                "Dimension mismatch: expected {} dimensions, found {} dimensions",
                point_dims, matrix_dims
            ));
        }
        
        let dot_products = points_matrix.dot(point);
        let point_norm = Self::l2_norm(point);
        let points_norms = Self::l2_norm_axis(points_matrix, Axis(1));
        let similarities = &dot_products / (&points_norms * point_norm);
        let distances = similarities.mapv(|sim| 1.0 - sim);        
        let min_distance = distances.fold(f32::MAX, |acc, &x| acc.min(x));
        
        if min_distance == f32::MAX {
            Ok(None)
        } else {
            Ok(Some(min_distance))
        }
    }

    fn l2_norm(vector: &Array1<f32>) -> f32 {
        vector.mapv(|x| x * x).sum().sqrt()
    }

    fn l2_norm_axis(matrix: &Array2<f32>, axis: Axis) -> Array1<f32> {
        matrix.mapv(|x| x * x).sum_axis(axis).mapv(|x| x.sqrt())
    }
} 

