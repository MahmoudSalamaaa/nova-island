//! fdlibm's pow (e_pow.c, Sun Microsystems 1993-2004), the one V8 uses for JavaScript's ** and
//! Math.pow. Windows' pow can differ in the last bit for large results; the smart clipboard shows
//! results to 16 digits, so 1.8 uses the same routine as 1.7.
//! ====================================================
//! Copyright (C) 2004 by Sun Microsystems, Inc. All rights reserved.
//! Permission to use, copy, modify, and distribute this software is freely granted, provided that
//! this notice is preserved.
//! ====================================================

fn hi(x: f64) -> i32 {
    (x.to_bits() >> 32) as i32
}
fn lo(x: f64) -> u32 {
    x.to_bits() as u32
}
fn with_hi(x: f64, h: i32) -> f64 {
    f64::from_bits(((h as u32 as u64) << 32) | lo(x) as u64)
}
fn from_words(h: i32, l: u32) -> f64 {
    f64::from_bits(((h as u32 as u64) << 32) | l as u64)
}
fn zero_lo(x: f64) -> f64 {
    f64::from_bits(x.to_bits() & 0xffff_ffff_0000_0000)
}

fn scalbn(x: f64, n: i32) -> f64 {
    let two54 = 1.80143985094819840000e+16;
    let twom54 = 5.55111512312578270212e-17;
    let huge: f64 = 1.0e+300;
    let tiny: f64 = 1.0e-300;
    let mut x = x;
    let mut hx = hi(x);
    let lx = lo(x);
    let mut k = (hx & 0x7ff00000) >> 20;
    if k == 0 {
        if (lx | (hx as u32 & 0x7fffffff)) == 0 {
            return x;
        }
        x *= two54;
        hx = hi(x);
        k = ((hx & 0x7ff00000) >> 20) - 54;
        if n < -50000 {
            return tiny * x;
        }
    }
    if k == 0x7ff {
        return x + x;
    }
    k += n;
    if k > 0x7fe {
        return huge * huge.copysign(x);
    }
    if k > 0 {
        return with_hi(x, (hx & !0x7ff00000) | (k << 20));
    }
    if k <= -54 {
        if n > 50000 {
            return huge * huge.copysign(x);
        }
        return tiny * tiny.copysign(x);
    }
    k += 54;
    with_hi(x, (hx & !0x7ff00000) | (k << 20)) * twom54
}

#[allow(clippy::excessive_precision)]
pub fn pow(x: f64, y: f64) -> f64 {
    const BP: [f64; 2] = [1.0, 1.5];
    const DP_H: [f64; 2] = [0.0, 5.84962487220764160156e-01];
    const DP_L: [f64; 2] = [0.0, 1.35003920212974897128e-08];
    const ZERO: f64 = 0.0;
    const ONE: f64 = 1.0;
    const TWO: f64 = 2.0;
    const TWO53: f64 = 9007199254740992.0;
    const HUGE: f64 = 1.0e300;
    const TINY: f64 = 1.0e-300;
    const L1: f64 = 5.99999999999994648725e-01;
    const L2: f64 = 4.28571428578550184252e-01;
    const L3: f64 = 3.33333329818377432918e-01;
    const L4: f64 = 2.72728123808534006489e-01;
    const L5: f64 = 2.30660745775561754067e-01;
    const L6: f64 = 2.06975017800338417784e-01;
    const P1: f64 = 1.66666666666666019037e-01;
    const P2: f64 = -2.77777777770155933842e-03;
    const P3: f64 = 6.61375632143793436117e-05;
    const P4: f64 = -1.65339022054652515390e-06;
    const P5: f64 = 4.13813679705723846039e-08;
    const LG2: f64 = 6.93147180559945286227e-01;
    const LG2_H: f64 = 6.93147182464599609375e-01;
    const LG2_L: f64 = -1.90465429995776804525e-09;
    const OVT: f64 = 8.0085662595372944372e-0017;
    const CP: f64 = 9.61796693925975554329e-01;
    const CP_H: f64 = 9.61796700954437255859e-01;
    const CP_L: f64 = -7.02846165095275826516e-09;
    const IVLN2: f64 = 1.44269504088896338700e+00;
    const IVLN2_H: f64 = 1.44269502162933349609e+00;
    const IVLN2_L: f64 = 1.92596299112661746887e-08;

    let hx = hi(x);
    let lx = lo(x);
    let hy = hi(y);
    let ly = lo(y);
    let mut ix = hx & 0x7fffffff;
    let iy = hy & 0x7fffffff;

    // y == 0: x**0 = 1
    if (iy as u32 | ly) == 0 {
        return ONE;
    }
    // NaN arguments (JavaScript: NaN, including 1 ** NaN)
    if ix > 0x7ff00000 || (ix == 0x7ff00000 && lx != 0) || iy > 0x7ff00000 || (iy == 0x7ff00000 && ly != 0) {
        return f64::NAN;
    }
    // yisint: 0 not an integer, 1 odd, 2 even (only when x < 0)
    let mut yisint = 0;
    if hx < 0 {
        if iy >= 0x43400000 {
            yisint = 2;
        } else if iy >= 0x3ff00000 {
            let k = (iy >> 20) - 0x3ff;
            if k > 20 {
                let j = ly >> (52 - k);
                if (j << (52 - k)) == ly {
                    yisint = 2 - (j & 1) as i32;
                }
            } else if ly == 0 {
                let j = iy >> (20 - k);
                if (j << (20 - k)) == iy {
                    yisint = 2 - (j & 1);
                }
            }
        }
    }
    // special y
    if ly == 0 {
        if iy == 0x7ff00000 {
            // y is +-inf
            if ((ix - 0x3ff00000) as u32 | lx) == 0 {
                return f64::NAN; // JavaScript: (+-1) ** +-Infinity is NaN
            } else if ix >= 0x3ff00000 {
                return if hy >= 0 { y } else { ZERO };
            } else {
                return if hy < 0 { -y } else { ZERO };
            }
        }
        if iy == 0x3ff00000 {
            return if hy < 0 { ONE / x } else { x };
        }
        if hy == 0x40000000 {
            return x * x;
        }
        if hy == 0x3fe00000 && hx >= 0 {
            return x.sqrt();
        }
    }
    let mut ax = x.abs();
    // special x
    if lx == 0 && (ix == 0x7ff00000 || ix == 0 || ix == 0x3ff00000) {
        let mut z = ax;
        if hy < 0 {
            z = ONE / z;
        }
        if hx < 0 {
            if ((ix - 0x3ff00000) | yisint) == 0 {
                z = (z - z) / (z - z);
            } else if yisint == 1 {
                z = -z;
            }
        }
        return z;
    }
    let mut n = (hx >> 31) + 1;
    // (x<0)**(non-int) is NaN
    if (n | yisint) == 0 {
        return (x - x) / (x - x);
    }
    let mut s = ONE;
    if (n | (yisint - 1)) == 0 {
        s = -ONE;
    }
    let (t1, t2);
    if iy > 0x41e00000 {
        // |y| > 2**31
        if iy > 0x43f00000 {
            if ix <= 0x3fefffff {
                return if hy < 0 { HUGE * HUGE } else { TINY * TINY };
            }
            if ix >= 0x3ff00000 {
                return if hy > 0 { HUGE * HUGE } else { TINY * TINY };
            }
        }
        if ix < 0x3fefffff {
            return if hy < 0 { s * HUGE * HUGE } else { s * TINY * TINY };
        }
        if ix > 0x3ff00000 {
            return if hy > 0 { s * HUGE * HUGE } else { s * TINY * TINY };
        }
        let t = ax - ONE;
        let w = (t * t) * (0.5 - t * (0.3333333333333333333333 - t * 0.25));
        let u = IVLN2_H * t;
        let v = t * IVLN2_L - w * IVLN2;
        let a = zero_lo(u + v);
        t1 = a;
        t2 = v - (a - u);
    } else {
        n = 0;
        if ix < 0x00100000 {
            ax *= TWO53;
            n -= 53;
            ix = hi(ax);
        }
        n += ((ix) >> 20) - 0x3ff;
        let j = ix & 0x000fffff;
        ix = j | 0x3ff00000;
        let k = if j <= 0x3988E {
            0
        } else if j < 0xBB67A {
            1
        } else {
            n += 1;
            ix -= 0x00100000;
            0
        };
        ax = with_hi(ax, ix);
        let u = ax - BP[k];
        let v = ONE / (ax + BP[k]);
        let ss = u * v;
        let s_h = zero_lo(ss);
        let t_h = from_words(((ix >> 1) | 0x20000000) + 0x00080000 + ((k as i32) << 18), 0);
        let t_l = ax - (t_h - BP[k]);
        let s_l = v * ((u - s_h * t_h) - s_h * t_l);
        let s2 = ss * ss;
        let mut r = s2 * s2 * (L1 + s2 * (L2 + s2 * (L3 + s2 * (L4 + s2 * (L5 + s2 * L6)))));
        r += s_l * (s_h + ss);
        let s2 = s_h * s_h;
        let t_h = zero_lo(3.0 + s2 + r);
        let t_l = r - ((t_h - 3.0) - s2);
        let u = s_h * t_h;
        let v = s_l * t_h + t_l * ss;
        let p_h = zero_lo(u + v);
        let p_l = v - (p_h - u);
        let z_h = CP_H * p_h;
        let z_l = CP_L * p_h + p_l * CP + DP_L[k];
        let t = n as f64;
        let a = zero_lo(((z_h + z_l) + DP_H[k]) + t);
        t1 = a;
        t2 = z_l - (((a - t) - DP_H[k]) - z_h);
    }
    // split y and compute (y1+y2)*(t1+t2)
    let y1 = zero_lo(y);
    let p_l = (y - y1) * t1 + y * t2;
    let mut p_h = y1 * t1;
    let mut z = p_l + p_h;
    let j = hi(z);
    let i = lo(z) as i32;
    if j >= 0x40900000 {
        if ((j - 0x40900000) | i) != 0 {
            return s * HUGE * HUGE;
        } else if p_l + OVT > z - p_h {
            return s * HUGE * HUGE;
        }
    } else if (j & 0x7fffffff) >= 0x4090cc00 {
        if ((j - 0xc090cc00u32 as i32) | i) != 0 {
            return s * TINY * TINY;
        } else if p_l <= z - p_h {
            return s * TINY * TINY;
        }
    }
    let i = j & 0x7fffffff;
    let mut k = (i >> 20) - 0x3ff;
    let mut n = 0;
    if i > 0x3fe00000 {
        n = j + (0x00100000 >> (k + 1));
        k = ((n & 0x7fffffff) >> 20) - 0x3ff;
        let t = from_words(n & !(0x000fffff >> k), 0);
        n = ((n & 0x000fffff) | 0x00100000) >> (20 - k);
        if j < 0 {
            n = -n;
        }
        p_h -= t;
    }
    let t = zero_lo(p_l + p_h);
    let u = t * LG2_H;
    let v = (p_l - (t - p_h)) * LG2 + t * LG2_L;
    z = u + v;
    let w = v - (z - u);
    let t = z * z;
    let t1 = z - t * (P1 + t * (P2 + t * (P3 + t * (P4 + t * P5))));
    let r = (z * t1) / (t1 - TWO) - (w + z * w);
    z = ONE - (r - z);
    let mut j = hi(z);
    j += n << 20;
    if (j >> 20) <= 0 {
        z = scalbn(z, n);
    } else {
        z = with_hi(z, j);
    }
    s * z
}
