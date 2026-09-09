//! A lightweight 3D inspection viewport for the compiled `StitchGraph`,
//! drawn with `egui::Painter` rather than a full 3D engine. Two render
//! modes share the same camera/projection math:
//!   - Wireframe: points + edges (good for seeing graph structure).
//!   - Shaded: flat-shaded quads/triangles stitched between consecutive
//!     rounds (a Painter's-algorithm software rasterizer - depth-sorted,
//!     lit with a single fixed directional light), which reads much more
//!     like an actual fabric surface than a point cloud.
//!
//! Swapping either of these for a real GPU mesh renderer (`three-d`, see
//! the commented note in `crates/app/Cargo.toml`) is still the natural
//! upgrade if you want lit/textured yarn geometry rather than a flat-shaded
//! inspection view - this gets you most of the *legibility* without the
//! wgpu/glow context-sharing work that would take.

use abyssal_thread_core::graph::TensionState;
use abyssal_thread_core::{StitchEdge, StitchGraph, Vec3};
use eframe::egui::{self, Color32, Pos2, Vec2};
use petgraph::graph::NodeIndex;
use petgraph::visit::EdgeRef;

pub struct ViewportState {
    pub yaw: f32,
    pub pitch: f32,
    pub distance: f32,
    pub target: Vec3,
    pub shaded: bool,
}

impl Default for ViewportState {
    fn default() -> Self {
        Self {
            yaw: 0.6,
            pitch: 0.4,
            distance: 200.0,
            target: Vec3::ZERO,
            shaded: true,
        }
    }
}

const FOV: f32 = 60.0_f32 * std::f32::consts::PI / 180.0;
/// Fixed world-space light direction for flat shading, arbitrary but
/// stable so the lighting doesn't spin along with the camera.
const LIGHT_DIR: Vec3 = Vec3 {
    x: 0.3,
    y: 0.4,
    z: 0.85,
};
const AMBIENT: f32 = 0.35;

fn camera_position(state: &ViewportState) -> Vec3 {
    let x = state.distance * state.pitch.cos() * state.yaw.sin();
    let y = state.distance * state.pitch.sin();
    let z = state.distance * state.pitch.cos() * state.yaw.cos();
    state.target + Vec3::new(x, y, z)
}

/// World -> camera space (right/up/forward basis centered on the eye).
fn to_camera_space(p: Vec3, state: &ViewportState) -> Vec3 {
    let eye = camera_position(state);
    let forward = (state.target - eye).normalized();
    let world_up = Vec3::new(0.0, 1.0, 0.0);
    let right = forward.cross(world_up).normalized();
    let up = right.cross(forward);
    let rel = p - eye;
    Vec3::new(rel.dot(right), rel.dot(up), rel.dot(forward))
}

/// Camera space -> screen space within `rect`. Returns `None` if the point
/// is behind or too close to the camera.
fn screen_from_cam(cam: Vec3, rect: egui::Rect) -> Option<Pos2> {
    if cam.z <= 1.0 {
        return None;
    }
    let aspect = rect.width() / rect.height().max(1.0);
    let f = 1.0 / (FOV / 2.0).tan();
    let ndc_x = (cam.x * f) / (cam.z * aspect);
    let ndc_y = (cam.y * f) / cam.z;
    Some(Pos2::new(
        rect.center().x + ndc_x * rect.width() * 0.5,
        rect.center().y - ndc_y * rect.height() * 0.5,
    ))
}

fn project(p: Vec3, state: &ViewportState, rect: egui::Rect) -> Option<Pos2> {
    screen_from_cam(to_camera_space(p, state), rect)
}

fn tension_color(t: Option<TensionState>) -> Color32 {
    match t {
        Some(TensionState::Normal) => Color32::from_rgb(90, 200, 90),
        Some(TensionState::Loose) => Color32::from_rgb(90, 150, 230),
        Some(TensionState::Stretched) => Color32::from_rgb(230, 90, 90),
        None => Color32::from_gray(180),
    }
}

/// Colorwork graphs (from image import) carry a real yarn color per stitch
/// - show that instead of the tension-state color when present, so the 3D
///   view actually looks like the picture rather than a uniform gray (which
///   is what every colorwork node would show under `tension_color` alone,
///   since tension analysis is skipped for flat-grid layouts).
fn node_display_color(color: Option<[u8; 3]>, tension: Option<TensionState>) -> Color32 {
    match color {
        Some([r, g, b]) => Color32::from_rgb(r, g, b),
        None => tension_color(tension),
    }
}

fn shade(color: Color32, factor: f32) -> Color32 {
    let f = factor.clamp(AMBIENT, 1.0);
    Color32::from_rgb(
        (color.r() as f32 * f) as u8,
        (color.g() as f32 * f) as u8,
        (color.b() as f32 * f) as u8,
    )
}

/// Builds the fabric surface as a list of 4-vertex faces connecting each
/// stitch to its round-neighbor and to their respective parents in the
/// previous round. Two adjacent stitches sharing one parent (e.g. the two
/// children of an `inc`) degenerate cleanly to a zero-area edge rather than
/// a crash. The first round (no parents yet) contributes no faces - it's
/// the ring/tip, left as bare points; capping it off is a reasonable
/// follow-up if you want a fully closed surface.
fn build_faces(g: &StitchGraph) -> Vec<[NodeIndex; 4]> {
    let mut faces = Vec::new();
    for round in &g.rounds {
        let n = round.len();
        if n < 2 {
            continue;
        }
        // Typical adjacent-stitch spacing in this round - used below to
        // detect a spurious "wrap" face on flat colorwork rows, which
        // aren't closed loops the way shaped-pattern rounds are.
        let typical_gap = (0..n - 1)
            .filter_map(|i| {
                let (x, y) = (round[i], round[i + 1]);
                match (g.graph[x].position, g.graph[y].position) {
                    (Some(px), Some(py)) => Some(px.distance(py)),
                    _ => None,
                }
            })
            .fold(0.0_f32, f32::max);

        for i in 0..n {
            let a = round[i];
            let b = round[(i + 1) % n];

            // Skip the wrap-around pair (last stitch -> first stitch) when
            // it's dramatically longer than a normal adjacent-stitch gap -
            // that means this round is a flat, open row (colorwork), not a
            // genuinely closed loop (shaped pattern), and connecting its
            // far ends would fabricate a face spanning the whole row. A
            // real closed round has every gap (including the wrap) roughly
            // equal, since the stitches are evenly spaced around it.
            if i == n - 1 {
                if let (Some(a_pos), Some(b_pos)) = (g.graph[a].position, g.graph[b].position) {
                    if typical_gap > 0.0 && a_pos.distance(b_pos) > typical_gap * 3.0 {
                        continue;
                    }
                }
            }

            if let (Some(pa), Some(pb)) = (g.parent_of(a), g.parent_of(b)) {
                faces.push([pa, a, b, pb]);
            }
        }
    }
    faces
}

/// Draws the viewport and handles orbit (drag) + zoom (scroll) input.
/// Returns `Some(node)` if a stitch point was clicked this frame, so the
/// grid editor can cross-highlight it. `selected` may contain more than one
/// node (a merged increase cell in the grid highlights both of its stitches).
pub fn show(
    ui: &mut egui::Ui,
    graph: Option<&StitchGraph>,
    state: &mut ViewportState,
    selected: &[NodeIndex],
) -> Option<NodeIndex> {
    let (rect, response) =
        ui.allocate_exact_size(ui.available_size(), egui::Sense::click_and_drag());

    if response.dragged() {
        let delta = response.drag_delta();
        state.yaw += delta.x * 0.01;
        state.pitch = (state.pitch + delta.y * 0.01).clamp(-1.5, 1.5);
    }
    let scroll = ui.input(|i| i.raw_scroll_delta.y);
    if scroll != 0.0 && response.hovered() {
        state.distance = (state.distance - scroll * 0.5).clamp(20.0, 2000.0);
    }

    let painter = ui.painter_at(rect);
    painter.rect_filled(rect, 0.0, Color32::from_gray(24));

    let Some(g) = graph else {
        painter.text(
            rect.center(),
            egui::Align2::CENTER_CENTER,
            "Compile a pattern to see it here",
            egui::FontId::default(),
            Color32::GRAY,
        );
        return None;
    };

    if state.shaded {
        let light = LIGHT_DIR.normalized();
        let mut draws: Vec<(f32, Vec<Pos2>, Color32)> = Vec::new();

        for face in build_faces(g) {
            let positions: Option<Vec<Vec3>> =
                face.iter().map(|&idx| g.graph[idx].position).collect();
            let Some(positions) = positions else { continue };
            let cams: Vec<Vec3> = positions
                .iter()
                .map(|&p| to_camera_space(p, state))
                .collect();
            if cams.iter().any(|c| c.z <= 1.0) {
                continue; // skip faces poking through the near plane
            }
            let screen: Option<Vec<Pos2>> =
                cams.iter().map(|&c| screen_from_cam(c, rect)).collect();
            let Some(screen) = screen else { continue };

            let edge1 = positions[1] - positions[0];
            let edge2 = positions[2] - positions[0];
            let normal = edge1.cross(edge2).normalized();
            // `abs()`: a thin fabric surface should look lit from either
            // side rather than going black when a face's winding points
            // away from the light.
            let brightness = normal.dot(light).abs();
            let base = node_display_color(g.graph[face[1]].color, g.graph[face[1]].tension);
            let color = shade(base, brightness.max(AMBIENT));

            let avg_z = cams.iter().map(|c| c.z).sum::<f32>() / cams.len() as f32;
            draws.push((avg_z, screen, color));
        }

        // Painter's algorithm: farthest first so nearer faces draw on top.
        draws.sort_by(|a, b| b.0.partial_cmp(&a.0).unwrap_or(std::cmp::Ordering::Equal));
        for (_, points, color) in draws {
            painter.add(egui::Shape::convex_polygon(
                points,
                color,
                egui::Stroke::NONE,
            ));
        }
    } else {
        for edge in g.graph.edge_references() {
            let (Some(pa), Some(pb)) = (
                g.graph[edge.source()].position,
                g.graph[edge.target()].position,
            ) else {
                continue;
            };
            if let (Some(sa), Some(sb)) = (project(pa, state, rect), project(pb, state, rect)) {
                let color = match edge.weight() {
                    StitchEdge::Sequence => Color32::from_gray(140),
                    StitchEdge::Parent => Color32::from_gray(70),
                };
                painter.line_segment([sa, sb], (1.0, color));
            }
        }
    }

    let mut clicked_node = None;
    let click_pos = response.interact_pointer_pos();

    for idx in g.graph.node_indices() {
        let node = &g.graph[idx];
        let Some(pos) = node.position else { continue };
        let Some(screen) = project(pos, state, rect) else {
            continue;
        };

        let is_selected = selected.contains(&idx);
        let radius = if is_selected { 6.0 } else { 4.0 };
        painter.circle_filled(screen, radius, node_display_color(node.color, node.tension));
        if is_selected {
            painter.circle_stroke(screen, radius + 2.0, (1.5, Color32::WHITE));
        }

        if response.clicked() {
            if let Some(click) = click_pos {
                if click.distance(screen) <= radius + 4.0 {
                    clicked_node = Some(idx);
                }
            }
        }
    }

    painter.text(
        rect.left_top() + Vec2::new(6.0, 4.0),
        egui::Align2::LEFT_TOP,
        "drag to orbit \u{b7} scroll to zoom",
        egui::FontId::proportional(10.0),
        Color32::from_gray(150),
    );

    clicked_node
}
