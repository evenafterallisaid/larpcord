use super::*;

fn particles(progress: f32) -> Vec<(f32, f32, f32, f32)> {
    if !(0.02..0.92).contains(&progress) {
        return Vec::new();
    }
    let flight = (progress / 0.92).clamp(0., 1.);
    (0..14)
        .map(|n| {
            let angle = n as f32 * std::f32::consts::TAU / 14. + 0.2;
            let reach = (8. + flight * 24.) * (0.7 + (n % 3) as f32 * 0.15);
            (
                angle.cos() * reach,
                angle.sin() * reach + flight * flight * 10.,
                (1. - flight) * 2.8 + 0.6,
                (1. - flight).sqrt(),
            )
        })
        .collect()
}

pub(super) fn overlay(color: u32, progress: f32) -> AnyElement {
    canvas(
        |_, _, _| (),
        move |bounds, _, window, _| {
            let center = point(
                bounds.origin.x + px(17.),
                bounds.origin.y + bounds.size.height / 2.,
            );
            if progress > 0.02 && progress < 0.8 {
                let radius = px(9. + progress * 27.);
                window.paint_quad(
                    outline(
                        Bounds::new(
                            center - point(radius, radius),
                            size(radius * 2., radius * 2.),
                        ),
                        rgba((color << 8) | ((1. - progress) * 110.) as u32),
                        BorderStyle::Solid,
                    )
                    .corner_radii(radius),
                );
            }
            for (x, y, radius, alpha) in particles(progress) {
                let quad = fill(
                    Bounds::new(
                        center + point(px(x - radius), px(y - radius)),
                        size(px(radius * 2.), px(radius * 2.)),
                    ),
                    rgba((color << 8) | ((alpha * 255.) as u32)),
                )
                .corner_radii(px(radius));
                window.paint_quad(quad);
            }
        },
    )
    .absolute()
    .inset_0()
    .size_full()
    .into_any_element()
}

#[cfg(test)]
mod tests {
    use super::particles;
    #[test]
    fn burst_particles_finish_and_remain_bounded() {
        assert!(particles(0.).is_empty());
        assert!(particles(1.).is_empty());
        let early = particles(0.15);
        let late = particles(0.7);
        assert_eq!(early.len(), 14);
        assert!(early[0].0 < late[0].0);
        assert!(early[0].3 > late[0].3);
        assert!(late.iter().all(|(x, y, r, a)| x.abs() < 40.
            && y.abs() < 45.
            && *r < 4.
            && *a >= 0.
            && *a <= 1.));
    }
}
