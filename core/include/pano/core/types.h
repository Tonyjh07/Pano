#pragma once

#include <QJsonValue>
#include <QString>
#include <QVector>

namespace pano {

// A single monitoring value. Mirrors the Rust `SampleValue` enum.
struct SampleValue {
    enum Type { Double, String, Json };

    Type type = Double;
    double number = 0.0;
    QString text;
    QJsonValue json;

    bool isNumber() const { return type == Double; }
    bool isString() const { return type == String; }

    static SampleValue fromNumber(double v) { SampleValue s; s.type = Double; s.number = v; return s; }
    static SampleValue fromString(QString v) { SampleValue s; s.type = String; s.text = std::move(v); return s; }
    static SampleValue fromJson(QJsonValue v) { SampleValue s; s.type = Json; s.json = std::move(v); return s; }
};

struct SerialSample;

// One point of one series at a time. Mirrors the Rust `Sample`.
struct Sample {
    QString series;
    qint64 timestampMs = 0;
    SampleValue value;
};

// Static description of a measurable series an adapter can emit.
struct SeriesInfo {
    QString id;                                // e.g. "sys.cpu.usage"
    QString label;                             // human label, e.g. "CPU 使用率"
    SampleValue::Type type = SampleValue::Double;
    QString unit;                              // e.g. "%", "B/s"
};

// Adapter capabilities. Mirrors the Rust `Capability`.
enum class Capability { TimeSeries, SystemInfo, RemoteSource };

QString capabilityName(Capability c);

// A declarative piece of an adapter's configuration schema (used to render forms).
struct ConfigField {
    QString key;
    QString label;
    bool numeric = true;      // numeric (min/max/def) if true, boolean otherwise
    double minVal = 0.0;
    double maxVal = 100.0;
    double defVal = 0.0;
    bool defBool = false;
};

} // namespace pano
