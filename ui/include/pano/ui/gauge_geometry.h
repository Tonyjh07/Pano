#pragma once

#include <QtGlobal>

namespace pano {

// Pure geometry math for the 240° car dashboard gauge. Kept free of widgets so it
// is directly unit-testable. Convention: value `minValue` is at 8 o'clock
// (-120°), mid is at 12 o'clock (0°), max at 4 o'clock (+120°). Angle is measured
// from "up", positive clockwise.
struct GaugeGeometry {
    static double angleFor(double value, double minValue, double maxValue) {
        if (maxValue <= minValue)
            return -120.0;
        double t = (value - minValue) / (maxValue - minValue);
        t = qBound(0.0, t, 1.0);
        return -120.0 + t * 240.0;
    }

    static double fraction(double value, double minValue, double maxValue) {
        if (maxValue <= minValue)
            return 0.0;
        return qBound(0.0, (value - minValue) / (maxValue - minValue), 1.0);
    }

    static bool overThreshold(double value, double threshold) {
        return value >= threshold;
    }
};

} // namespace pano
