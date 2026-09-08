//! Exports the laid-out stitch graph as a Wavefront OBJ point/line cloud -
//! one vertex per stitch, one line per Sequence/Parent connection. This is
//! deliberately minimal (no actual stitch mesh geometry yet); it's meant as
//! an armature you can import into Blender and build a proper yarn mesh
//! around (e.g. via a skin modifier along the `l` edges), not a final render.

use abyssal_thread_core::StitchGraph;
use petgraph::visit::EdgeRef;
use std::collections::HashMap;

pub fn export_obj(g: &StitchGraph) -> String {
    let mut out = String::from("# abyssal-thread stitch armature\no pattern\n");
    let mut index_map: HashMap<_, usize> = HashMap::new();

    for (i, node_idx) in g.graph.node_indices().enumerate() {
        let pos = g.graph[node_idx].position.unwrap_or(abyssal_thread_core::Vec3::ZERO);
        // OBJ vertex coordinates are in meters by convention here; our
        // baseline dimensions are millimeters, so scale down.
        out.push_str(&format!("v {:.4} {:.4} {:.4}\n", pos.x / 1000.0, pos.y / 1000.0, pos.z / 1000.0));
        index_map.insert(node_idx, i + 1); // OBJ indices are 1-based
    }

    for edge in g.graph.edge_references() {
        let a = index_map[&edge.source()];
        let b = index_map[&edge.target()];
        out.push_str(&format!("l {a} {b}\n"));
    }

    out
}
