use abyssal_thread_lang::compile;

fn round_sizes(src: &str) -> Vec<usize> {
    let g = compile(src).expect("pattern should compile");
    g.rounds.iter().map(|r| r.len()).collect()
}

#[test]
fn sphere_pattern_has_expected_round_shaping() {
    let src = include_str!("../../../examples/sphere.cgp");
    let sizes = round_sizes(src);
    assert_eq!(sizes, vec![6, 12, 18, 24, 24, 24, 18, 12, 6]);
}

#[test]
fn motif_pattern_compiles_and_attaches_to_label() {
    let src = include_str!("../../../examples/motif_with_attachment.cgp");
    let g = compile(src).expect("pattern should compile");
    assert_eq!(g.rounds.len(), 3);
    assert_eq!(g.rounds[0].len(), 6);
    assert_eq!(g.rounds[1].len(), 6);
    assert_eq!(g.rounds[2].len(), 6);

    let anchor_idx = *g.labels.get("anchor").expect("anchor label should exist");
    assert_eq!(g.graph[anchor_idx].round, 1);

    // Round 2's first stitch was written as `@anchor` - confirm it actually
    // attached to the labeled node rather than positionally.
    let first_of_round_2 = g.rounds[2][0];
    assert_eq!(g.parent_of(first_of_round_2), Some(anchor_idx));
}

#[test]
fn shell_stitch_pattern_expands_custom_stitch_alias() {
    let src = include_str!("../../../examples/shell_stitch.cgp");
    let sizes = round_sizes(src);
    // 2 shells * 3dc each = 6 stitches, consuming the 6 parents from round 0.
    assert_eq!(sizes, vec![6, 6]);
}
