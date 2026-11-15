//! Java 1.12 与 Bedrock legacy 方块 state 的双向映射。

use std::sync::OnceLock;

const STATES: usize = 256 * 16;
static TO_NATIVE: OnceLock<Box<[i32; STATES]>> = OnceLock::new();
static TO_JAVA: OnceLock<Box<[i32; STATES]>> = OnceLock::new();

fn map(native: &mut [i32; STATES], java: &mut [i32; STATES], java_state: i32, native_state: i32) {
    native[java_state as usize] = native_state;
    java[native_state as usize] = java_state;
}

fn tables() -> (&'static [i32; STATES], &'static [i32; STATES]) {
    // 两张表必须一起初始化，反向表里有几条没有正向对应的特例。
    TO_NATIVE.get_or_init(|| {
        let mut to_native = Box::new(std::array::from_fn(|state| state as i32));
        let mut to_java = Box::new(std::array::from_fn(|state| state as i32));

        for (java, native) in [
            (95, 241),
            (125, 157),
            (126, 158),
            (157, 126),
            (158, 125),
            (198, 208),
            (199, 240),
            (208, 198),
            (212, 207),
        ] {
            for meta in 0..16 {
                map(
                    &mut to_native,
                    &mut to_java,
                    (java << 4) | meta,
                    (native << 4) | meta,
                );
            }
        }
        map(&mut to_native, &mut to_java, (3 << 4) | 2, 243 << 4);
        map(&mut to_native, &mut to_java, 204 << 4, (181 << 4) | 1);
        for half in [0, 8] {
            map(
                &mut to_native,
                &mut to_java,
                (205 << 4) | half,
                (182 << 4) | 1 | half,
            );
        }
        for (id, wood) in [(188, 1), (189, 2), (190, 3), (191, 5), (192, 4)] {
            map(&mut to_native, &mut to_java, id << 4, (85 << 4) | wood);
        }
        for meta in 0..16 {
            if (meta & 7) < 6 {
                for id in [77, 143] {
                    let facing = [0, 5, 4, 3, 2, 1][(meta & 7) as usize];
                    map(
                        &mut to_native,
                        &mut to_java,
                        (id << 4) | meta,
                        (id << 4) | (meta & 8) | facing,
                    );
                }
            }
            for id in [96, 167] {
                map(
                    &mut to_native,
                    &mut to_java,
                    (id << 4) | meta,
                    (id << 4) | (3 - (meta & 3)) | ((meta & 4) << 1) | ((meta & 8) >> 1),
                );
            }
            for id in [43, 44] {
                if (meta & 7) >= 6 {
                    map(
                        &mut to_native,
                        &mut to_java,
                        (id << 4) | meta,
                        (id << 4) | (meta ^ 1),
                    );
                }
            }
            if (meta & 3) == 1 || (meta & 3) == 2 {
                map(
                    &mut to_native,
                    &mut to_java,
                    (168 << 4) | meta,
                    (168 << 4) | (meta ^ 3),
                );
            }
            if (meta & 8) != 0 {
                // 高半格在 Bedrock 中携带植物种类，回到 Java 时只保留 half 位。
                to_java[((175 << 4) | meta) as usize] = (175 << 4) | 8;
            }
        }
        for (age, growth) in [(0, 0), (1, 2), (2, 4), (3, 7)] {
            map(
                &mut to_native,
                &mut to_java,
                (207 << 4) | age,
                (244 << 4) | growth,
            );
        }
        for meta in 0..8 {
            to_java[((244 << 4) | meta) as usize] =
                (207 << 4) | if meta == 7 { 3 } else { meta / 2 };
        }

        TO_JAVA
            .set(to_java)
            .ok()
            .expect("Java 反向映射只初始化一次");
        to_native
    });
    (TO_NATIVE.get().unwrap(), TO_JAVA.get().unwrap())
}

#[inline]
pub fn to_native(state: i32) -> i32 {
    tables().0.get(state as usize).copied().unwrap_or(state)
}

#[inline]
pub fn to_java(state: i32) -> i32 {
    tables().1.get(state as usize).copied().unwrap_or(state)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn mapped_ids_and_metadata_round_trip() {
        for (java, native) in [
            (95, 241),
            (125, 157),
            (126, 158),
            (157, 126),
            (158, 125),
            (198, 208),
            (199, 240),
            (208, 198),
            (212, 207),
        ] {
            for meta in 0..16 {
                let java_state = (java << 4) | meta;
                let native_state = (native << 4) | meta;
                assert_eq!(to_native(java_state), native_state);
                assert_eq!(to_java(native_state), java_state);
            }
        }
        assert_eq!(to_native((3 << 4) | 2), 243 << 4);
        assert_eq!(to_java(243 << 4), (3 << 4) | 2);
        assert_eq!(to_native(188 << 4), (85 << 4) | 1);
        assert_eq!(to_native(192 << 4), (85 << 4) | 4);
    }

    #[test]
    fn special_button_trapdoor_slab_and_prismarine_states() {
        for id in [77, 143] {
            for meta in 0..6 {
                let expected = (id << 4) | [0, 5, 4, 3, 2, 1][meta];
                assert_eq!(to_native((id << 4) | meta as i32), expected);
                assert_eq!(to_java(expected), (id << 4) | meta as i32);
            }
        }
        assert_eq!(to_native(96 << 4), (96 << 4) | 3);
        assert_eq!(to_native((167 << 4) | 15), (167 << 4) | 12);
        assert_eq!(to_native((44 << 4) | 6), (44 << 4) | 7);
        assert_eq!(to_native((168 << 4) | 1), (168 << 4) | 2);
    }

    #[test]
    fn asymmetric_plant_and_beetroot_reverse_states() {
        assert_eq!(to_java((175 << 4) | 9), (175 << 4) | 8);
        assert_eq!(to_native((207 << 4) | 1), (244 << 4) | 2);
        assert_eq!(to_native((207 << 4) | 3), (244 << 4) | 7);
        for (native, java) in [
            (0, 0),
            (1, 0),
            (2, 1),
            (3, 1),
            (4, 2),
            (5, 2),
            (6, 3),
            (7, 3),
        ] {
            assert_eq!(to_java((244 << 4) | native), (207 << 4) | java);
        }
        assert_eq!(to_native(1 << 4), 1 << 4);
        assert_eq!(to_java(1 << 4), 1 << 4);
    }
}
