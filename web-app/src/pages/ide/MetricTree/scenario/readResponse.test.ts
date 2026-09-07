import { describe, expect, it } from "vitest";
import type { FittedDriver } from "@/types/metricTree";
import { readResponse } from "./readResponse";

// Real engine output for example_new, so these assert against what the server
// actually sends rather than a curve shaped to make the test pass. Regenerate with
// `airlayer predict --if store_days.<measure>=1 --time store_days.business_date
//  --period 2025-07-20:2026-07-19 --json`, reading `.fitted[].profile`.
const TURNING: [number, number][] = [
  [-0.848485, -82857.4845],
  [-0.824747, -79327.3491],
  [-0.80101, -75866.9853],
  [-0.777273, -72476.3934],
  [-0.753535, -69155.5732],
  [-0.729798, -65904.5247],
  [-0.706061, -62723.248],
  [-0.682323, -59611.7431],
  [-0.658586, -56570.0099],
  [-0.634848, -53598.0485],
  [-0.611111, -50695.8588],
  [-0.587374, -47863.4409],
  [-0.563636, -45100.7947],
  [-0.539899, -42407.9203],
  [-0.516162, -39784.8177],
  [-0.492424, -37231.4868],
  [-0.468687, -34747.9277],
  [-0.444949, -32334.1403],
  [-0.421212, -29990.1247],
  [-0.397475, -27715.8808],
  [-0.373737, -25511.4087],
  [-0.35, -23376.7084],
  [-0.326263, -21311.7798],
  [-0.302525, -19316.623],
  [-0.278788, -17391.2379],
  [-0.255051, -15535.6246],
  [-0.231313, -13749.783],
  [-0.207576, -12033.7132],
  [-0.183838, -10387.4152],
  [-0.160101, -8810.8889],
  [-0.136364, -7304.1343],
  [-0.112626, -5867.1515],
  [-0.088889, -4499.9405],
  [-0.065152, -3202.5013],
  [-0.041414, -1974.8338],
  [-0.017677, -816.938],
  [0.006061, 271.186],
  [0.029798, 1289.5382],
  [0.053535, 2238.1187],
  [0.077273, 3116.9274],
  [0.10101, 3925.9644],
  [0.124747, 4665.2296],
  [0.148485, 5334.7231],
  [0.172222, 5934.4448],
  [0.19596, 6464.3947],
  [0.219697, 6924.5729],
  [0.243434, 7314.9793],
  [0.267172, 7635.614],
  [0.290909, 7886.4769],
  [0.314646, 8067.5681],
  [0.338384, 8178.8875],
  [0.362121, 8220.4351],
  [0.385859, 8192.211],
  [0.409596, 8094.2151],
  [0.433333, 7926.4475],
  [0.457071, 7688.9081],
  [0.480808, 7381.597],
  [0.504545, 7004.5141],
  [0.528283, 6557.6594],
  [0.55202, 6041.033],
  [0.575758, 5454.6349],
  [0.599495, 4798.4649],
  [0.623232, 4072.5233],
  [0.64697, 3276.8098],
  [0.670707, 2411.3246],
  [0.694444, 1476.0677],
  [0.718182, 471.039],
  [0.741919, -603.7615],
  [0.765657, -1748.3337],
  [0.789394, -2962.6777],
  [0.813131, -4246.7934],
  [0.836869, -5600.6809],
  [0.860606, -7024.3401],
  [0.884343, -8517.7711],
  [0.908081, -10080.9739],
  [0.931818, -11713.9484],
  [0.955556, -13416.6947],
  [0.979293, -15189.2127],
  [1.00303, -17031.5025],
  [1.026768, -18943.564],
  [1.050505, -20925.3973],
  [1.074242, -22977.0024],
  [1.09798, -25098.3792],
  [1.121717, -27289.5278],
  [1.145455, -29550.4481],
  [1.169192, -31881.1402],
  [1.192929, -34281.604],
  [1.216667, -36751.8396],
  [1.240404, -39291.847],
  [1.264141, -41901.6261],
  [1.287879, -44581.177],
  [1.311616, -47330.4996],
  [1.335354, -50149.594],
  [1.359091, -53038.4601],
  [1.382828, -55997.098],
  [1.406566, -59025.5076],
  [1.430303, -62123.689],
  [1.45404, -65291.6422],
  [1.477778, -68529.3671],
  [1.501515, -71836.8638],
  [1.525253, -75214.1322],
  [1.54899, -78661.1724],
  [1.572727, -82177.9844],
  [1.596465, -85764.5681],
  [1.620202, -89420.9235],
  [1.643939, -93147.0507],
  [1.667677, -96942.9497],
  [1.691414, -100808.6205],
  [1.715152, -104744.0629],
  [1.738889, -108749.2772],
  [1.762626, -112824.2632],
  [1.786364, -116969.0209],
  [1.810101, -121183.5504],
  [1.833838, -125467.8517],
  [1.857576, -129821.9247],
  [1.881313, -134245.7695],
  [1.905051, -138739.3861],
  [1.928788, -143302.7744],
  [1.952525, -147935.9344],
  [1.976263, -152638.8662],
  [2.0, -157411.5698]
];

const SATURATING: [number, number][] = [
  [-0.875809, -175877.2719],
  [-0.851642, -166473.6841],
  [-0.827476, -157879.2039],
  [-0.803309, -149923.6738],
  [-0.779143, -142488.8709],
  [-0.754976, -135488.7701],
  [-0.73081, -128858.5017],
  [-0.706643, -122547.739],
  [-0.682477, -116516.5226],
  [-0.65831, -110732.5074],
  [-0.634144, -105169.0819],
  [-0.609977, -99804.0446],
  [-0.585811, -94618.648],
  [-0.561645, -89596.8934],
  [-0.537478, -84724.9999],
  [-0.513312, -79990.9985],
  [-0.489145, -75384.4165],
  [-0.464979, -70896.0289],
  [-0.440812, -66517.6613],
  [-0.416646, -62242.0297],
  [-0.392479, -58062.6113],
  [-0.368313, -53973.5383],
  [-0.344146, -49969.5092],
  [-0.31998, -46045.7161],
  [-0.295814, -42197.7825],
  [-0.271647, -38421.7115],
  [-0.247481, -34713.8412],
  [-0.223314, -31070.8074],
  [-0.199148, -27489.51],
  [-0.174981, -23967.0861],
  [-0.150815, -20500.8846],
  [-0.126648, -17088.4453],
  [-0.102482, -13727.4804],
  [-0.078315, -10415.8581],
  [-0.054149, -7151.588],
  [-0.029982, -3932.8084],
  [-0.005816, -757.7752],
  [0.01835, 2375.1484],
  [0.042517, 5467.5007],
  [0.066683, 8520.7297],
  [0.09085, 11536.2004],
  [0.115016, 14515.2009],
  [0.139183, 17458.9488],
  [0.163349, 20368.596],
  [0.187516, 23245.2339],
  [0.211682, 26089.8976],
  [0.235849, 28903.5697],
  [0.260015, 31687.1841],
  [0.284181, 34441.6294],
  [0.308348, 37167.7517],
  [0.332514, 39866.3575],
  [0.356681, 42538.216],
  [0.380847, 45184.062],
  [0.405014, 47804.5974],
  [0.42918, 50400.4934],
  [0.453347, 52972.3926],
  [0.477513, 55520.9105],
  [0.50168, 58046.6368],
  [0.525846, 60550.1374],
  [0.550013, 63031.9553],
  [0.574179, 65492.612],
  [0.598345, 67932.6086],
  [0.622512, 70352.4272],
  [0.646678, 72752.5315],
  [0.670845, 75133.3679],
  [0.695011, 77495.3665],
  [0.719178, 79838.9418],
  [0.743344, 82164.4935],
  [0.767511, 84472.4071],
  [0.791677, 86763.0549],
  [0.815844, 89036.7964],
  [0.84001, 91293.9789],
  [0.864176, 93534.9381],
  [0.888343, 95759.9988],
  [0.912509, 97969.4749],
  [0.936676, 100163.6707],
  [0.960842, 102342.8803],
  [0.985009, 104507.3891],
  [1.009175, 106657.4735],
  [1.033342, 108793.4012],
  [1.057508, 110915.4321],
  [1.081675, 113023.8183],
  [1.105841, 115118.8045],
  [1.130008, 117200.6281],
  [1.154174, 119269.5197],
  [1.17834, 121325.7034],
  [1.202507, 123369.397],
  [1.226673, 125400.8122],
  [1.25084, 127420.1548],
  [1.275006, 129427.625],
  [1.299173, 131423.4177],
  [1.323339, 133407.7224],
  [1.347506, 135380.7237],
  [1.371672, 137342.6015],
  [1.395839, 139293.5308],
  [1.420005, 141233.6822],
  [1.444171, 143163.2221],
  [1.468338, 145082.3124],
  [1.492504, 146991.1115],
  [1.516671, 148889.7732],
  [1.540837, 150778.4482],
  [1.565004, 152657.2832],
  [1.58917, 154526.4214],
  [1.613337, 156386.0027],
  [1.637503, 158236.1636],
  [1.66167, 160077.0376],
  [1.685836, 161908.7549],
  [1.710003, 163731.4429],
  [1.734169, 165545.226],
  [1.758335, 167350.2259],
  [1.782502, 169146.5614],
  [1.806668, 170934.3489],
  [1.830835, 172713.7022],
  [1.855001, 174484.7326],
  [1.879168, 176247.5488],
  [1.903334, 178002.2575],
  [1.927501, 179748.9631],
  [1.951667, 181487.7675],
  [1.975834, 183218.7709],
  [2.0, 184942.0711]
];

/** A fitted driver carrying just the sampled response `readResponse` reads.
 *
 *  Named for what it builds, and deliberately not `fit` or `it` — both are
 *  vitest globals, and shadowing either makes every call in this file look to
 *  the collector like a test declared inside a test. */
const withProfile = (
  profile: [number, number][],
  extra: Partial<FittedDriver> = {}
): FittedDriver => ({
  from: "store_days.discount_depth",
  to: "store_days.promo_margin",
  profile,
  ...extra
});

describe("readResponse", () => {
  // The behaviour the whole exercise was for, found without the reader knowing
  // which shape the engine picked.
  it("finds an interior peak and where the lever turns harmful", () => {
    const r = readResponse(withProfile(TURNING));
    expect(r.peak).toBeDefined();
    // The engine's closed-form vertex for this fit is +36.4 percent; three-point
    // refinement recovers it from a 2.4-point grid.
    expect(r.peak as number).toBeCloseTo(0.364, 2);
    expect(r.breakEven as number).toBeCloseTo(0.729, 2);
    expect(r.peakDelta as number).toBeGreaterThan(0);
  });

  // A saturating curve rises for ever, so there is no ceiling to report. Quoting
  // the largest sampled lever as "best" would invent a recommendation out of where
  // the sampling happened to stop.
  it("reports no peak for a response that only ever rises", () => {
    const r = readResponse(withProfile(SATURATING));
    expect(r.peak).toBeUndefined();
    expect(r.breakEven).toBeUndefined();
    expect(r.saturating).toBe(true);
  });

  // A straight line also rises for ever, so the step size is what tells the two
  // apart — not a shape name.
  it("distinguishes a straight line from a saturating curve", () => {
    const line: [number, number][] = Array.from({ length: 40 }, (_, i) => {
      const lever = (i + 1) * 0.05;
      return [lever, 1000 * lever];
    });
    const r = readResponse(withProfile(line));
    expect(r.peak).toBeUndefined();
    expect(r.saturating).toBe(false);
  });

  // A refused or unfitted edge carries no profile; the reader must say nothing
  // rather than guess.
  it("says nothing when there is no profile", () => {
    expect(readResponse(withProfile([])).samples).toHaveLength(0);
    expect(readResponse({ from: "a", to: "b" }).peak).toBeUndefined();
  });

  // Nothing here mentions a form, so a shape the reader has never heard of still
  // reads correctly. This is the property the per-form switch could not have.
  it("reads a shape it has no name for", () => {
    const cubic: [number, number][] = Array.from({ length: 60 }, (_, i) => {
      const lever = (i + 1) * 0.05;
      return [lever, -100 * (lever - 0.5) * (lever - 1.5) * (lever + 1)];
    });
    const r = readResponse(withProfile(cubic));
    expect(r.peak).toBeDefined();
    expect(r.peak as number).toBeGreaterThan(0.5);
    expect(r.peak as number).toBeLessThan(1.5);
  });
});

describe("readResponse — a lever that hurts", () => {
  const withProfile = (profile: [number, number][]): FittedDriver => ({
    from: "store_days.discount_depth",
    to: "store_days.promo_margin",
    profile
  });

  // The bug this exists to pin: `lastStep < firstStep * 0.9` has no sign
  // guard, so two equal steps of −10 satisfy it (−10 < −9) and the panel
  // reported "each further increase buys less than the last" about a lever
  // that was lowering the target the whole way down.
  it("does not call a straight-line decline saturating", () => {
    const declining: [number, number][] = Array.from({ length: 20 }, (_, i) => {
      const lever = (i + 1) * 0.05;
      return [lever, -1000 * lever];
    });
    const r = readResponse(withProfile(declining));
    expect(r.saturating).toBe(false);
    expect(r.declining).toBe(true);
  });

  // The sign guard on `firstStep` alone does not reach this: the first step
  // *within* a curve that never leaves negative territory is positive, so
  // −100, −50, −80 satisfied `firstStep > 0` and reported diminishing returns
  // about a lever that is harmful at every sampled point. `declining` is
  // computed as `!saturating && …`, so it lost the tie and the harmful-lever
  // copy never rendered.
  it("does not call a rise-then-fall curve that stays negative saturating", () => {
    const r = readResponse(
      withProfile([
        [0.05, -100],
        [0.1, -50],
        [0.15, -80]
      ])
    );
    expect(r.saturating).toBe(false);
    expect(r.declining).toBe(true);
    // The interior maximum is still not a recommendation: it is below baseline.
    expect(r.peak).toBeUndefined();
  });

  it("still calls a genuinely saturating rise saturating", () => {
    const saturating: [number, number][] = Array.from({ length: 20 }, (_, i) => {
      const lever = (i + 1) * 0.05;
      return [lever, 1000 * Math.log1p(lever)];
    });
    const r = readResponse(withProfile(saturating));
    expect(r.saturating).toBe(true);
    expect(r.declining).toBe(false);
  });

  // `cubic` is an S-curve, and the two-point test cannot see one: flat, then
  // steep, then flat gives a small first step and a small last step, so
  // `lastStep < firstStep * 0.9` is satisfied while "each further increase buys
  // less than the last" is false right through the steep middle.
  it("does not call an S-curve saturating", () => {
    const s: [number, number][] = Array.from({ length: 20 }, (_, i) => {
      const lever = (i + 1) * 0.05;
      // Logistic: nearly flat at both ends, steepest in the middle.
      return [lever, 1000 / (1 + Math.exp(-12 * (lever - 0.5)))];
    });
    const r = readResponse(withProfile(s));
    expect(r.saturating).toBe(false);
    expect(r.declining).toBe(false);
  });

  // A cubic can turn twice, so it can drop through the baseline and come back.
  // "Past +X% it stops paying for itself" is false about a curve that pays
  // again further out, so the panel must say nothing rather than pick the first
  // crossing and present it as the break-even.
  it("refuses a break-even on a curve that crosses back", () => {
    // Positive, down through zero around 0.4, back up through it around 0.8.
    const twice: [number, number][] = Array.from({ length: 20 }, (_, i) => {
      const lever = (i + 1) * 0.05;
      return [lever, 1000 * (lever - 0.4) * (lever - 0.8) * (lever + 1)];
    });
    const r = readResponse(withProfile(twice));
    expect(r.breakEven).toBeUndefined();
  });

  // A curve that starts positive and crosses down is neither: it has a
  // break-even, which is a stronger statement than either flag.
  it("reports a break-even on a curve that turns negative", () => {
    const crossing: [number, number][] = Array.from({ length: 20 }, (_, i) => {
      const lever = (i + 1) * 0.05;
      return [lever, 1000 * (0.5 - lever)];
    });
    const r = readResponse(withProfile(crossing));
    expect(r.declining).toBe(false);
    expect(r.breakEven).toBeDefined();
    expect(r.breakEven as number).toBeCloseTo(0.5, 2);
  });
});
