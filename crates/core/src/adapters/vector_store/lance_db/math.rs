use ndarray::{Array1, Array2, Axis};
use crate::adapters::vector_store::types::Embedding;

pub(super) struct MathUtils;

impl MathUtils {


    pub(super) fn find_min_distance(
        point: &Embedding,
        points: &[Embedding],
    ) -> Result<Option<f32>, String> {
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
            .map_err(|e| format!("Failed to create matrix from points: {e}"))?;
        let point_array = Array1::from_vec(point.to_vec());
        Self::find_min_distance_from_matrix(&point_array, &points_matrix)
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
                "Dimension mismatch: expected {point_dims} dimensions, found {matrix_dims} dimensions"
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
