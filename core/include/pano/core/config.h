#pragma once

#include <QMap>
#include <QString>
#include <QVariant>

namespace pano {

// A minimal TOML-subset configuration loader. Mirrors the Rust `Config` loader
// for the sections Pano uses: `[core]`, `[ui]`, `[window.<id>]`, `[adapters."<id>"]`.
// Supported value types: string, number, boolean, and arrays of those.
class Config {
public:
    // Parses `text` (utf-8). On failure returns a default Config and fills `error`.
    static Config fromToml(const QString& text, QString* error = nullptr);

    // Look up a primitive value (string/double/bool) in a section.
    QVariant get(const QString& section, const QString& key, const QVariant& def = QVariant()) const;

    QString str(const QString& section, const QString& key, const QString& def = QString()) const;
    double num(const QString& section, const QString& key, double def = 0.0) const;
    bool boolean(const QString& section, const QString& key, bool def = false) const;
    QStringList strList(const QString& section, const QString& key) const;

    bool hasSection(const QString& section) const;
    bool isEmpty() const { return m_sections_.isEmpty(); }

    // All sections and all keys/sections-keys (for inspection / persistence).
    QMap<QString, QMap<QString, QVariant>> sections() const { return m_sections_; }
    void setSection(const QString& section, QMap<QString, QVariant> values) { m_sections_[section] = std::move(values); }

private:
    QMap<QString, QMap<QString, QVariant>> m_sections_;
};

} // namespace pano
