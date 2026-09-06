#include "pano/core/config.h"

#include <QJsonArray>
#include <QRegularExpression>

namespace pano {
namespace {

QString stripComment(const QString& line) {
    bool inStr = false;
    for (int i = 0; i < line.size(); ++i) {
        const QChar c = line[i];
        if (c == u'\"') {
            if (inStr && i > 0 && line[i - 1] == u'\\')
                continue;
            inStr = !inStr;
        } else if (c == u'#' && !inStr) {
            return line.left(i);
        }
    }
    return line;
}

QString unquote(QString raw, QString* err) {
    raw = raw.trimmed();
    if (raw.size() < 2 || raw.front() != u'\"' || raw.back() != u'\"') {
        if (err) *err = "invalid string: " + raw;
        return {};
    }
    QString core = raw.mid(1, raw.size() - 2);
    core.replace(QStringLiteral("\\\""), QStringLiteral("\""));
    core.replace(QStringLiteral("\\\\"), QStringLiteral("\\"));
    core.replace(QStringLiteral("\\n"), QStringLiteral("\n"));
    core.replace(QStringLiteral("\\t"), QStringLiteral("\t"));
    return core;
}

bool parseValue(const QString& raw, QVariant* out, QString* err) {
    QString v = raw.trimmed();
    if (v.isEmpty()) {
        if (err) *err = "empty value";
        return false;
    }

    if (v.startsWith(u'\"'))
        { *out = unquote(v, err); return err->isEmpty(); }

    if (v == QStringLiteral("true")) { *out = true; return true; }
    if (v == QStringLiteral("false")) { *out = false; return true; }

    if (v.startsWith(u'[')) {
        if (!v.endsWith(u']')) { if (err) *err = "unterminated array: " + v; return false; }
        QString inner = v.mid(1, v.size() - 2);
        QStringList parts;
        QString cur;
        int depth = 0;
        bool inStr = false;
        for (int i = 0; i < inner.size(); ++i) {
            const QChar c = inner[i];
            if (c == u'\"') {
                if (inStr && i > 0 && inner[i - 1] == u'\\') { cur += c; continue; }
                inStr = !inStr; cur += c;
            } else if ((c == u',' || c == u'\n') && !inStr && depth == 0) {
                if (!cur.trimmed().isEmpty()) parts << cur.trimmed();
                cur.clear();
            } else {
                cur += c;
            }
        }
        if (!cur.trimmed().isEmpty()) parts << cur.trimmed();
        QVariantList list;
        for (const QString& p : parts) {
            QVariant item;
            QString e2;
            if (!parseValue(p, &item, &e2)) { if (err) *err = "bad array item: " + p; return false; }
            list.push_back(item);
        }
        *out = list;
        return true;
    }

    // number
    static const QRegularExpression numRe(QStringLiteral(R"(^[-+]?[0-9]*\.?[0-9]+([eE][-+]?[0-9]+)?$)"));
    if (v.contains(u'.') || v.contains(u'e') || v.contains(u'E')) {
        if (!numRe.match(v).hasMatch()) { if (err) *err = "invalid number: " + v; return false; }
        *out = v.toDouble();
        return true;
    }
    bool ok = false;
    const long long iv = v.toLongLong(&ok);
    if (ok) { *out = qlonglong(iv); return true; }
    if (numRe.match(v).hasMatch()) { *out = v.toDouble(); return true; }
    if (err) *err = "unknown value: " + v;
    return false;
}

} // namespace

Config Config::fromToml(const QString& text, QString* error) {
    Config cfg;
    QString currentSection;
    const QStringList lines = text.split(u'\n');
    for (int ln = 0; ln < lines.size(); ++ln) {
        QString line = stripComment(lines[ln]).trimmed();
        if (line.isEmpty())
            continue;

        if (line.startsWith(u'[') && line.endsWith(u']')) {
            currentSection = line.mid(1, line.size() - 2).trimmed();
            // normalize `adapters."sys.cpu"` -> adapters.sys.cpu
            currentSection.replace(u'\"', QString());
            continue;
        }

        const int eq = line.indexOf(u'=');
        if (eq < 0) {
            if (error) *error = QStringLiteral("line %1: expected '='").arg(ln + 1);
            return {};
        }
        const QString key = line.left(eq).trimmed();
        const QString rawValue = line.mid(eq + 1).trimmed();
        if (key.isEmpty()) {
            if (error) *error = QStringLiteral("line %1: empty key").arg(ln + 1);
            return {};
        }

        QVariant value;
        QString perr;
        if (!parseValue(rawValue, &value, &perr)) {
            if (error) *error = QStringLiteral("line %1: %2").arg(ln + 1).arg(perr);
            return {};
        }
        cfg.m_sections_[currentSection][key] = value;
    }
    return cfg;
}

QVariant Config::get(const QString& section, const QString& key, const QVariant& def) const {
    const auto s = m_sections_.constFind(section);
    if (s == m_sections_.constEnd())
        return def;
    return s->value(key, def);
}

QString Config::str(const QString& section, const QString& key, const QString& def) const {
    const QVariant v = get(section, key);
    return v.isNull() ? def : v.toString();
}

double Config::num(const QString& section, const QString& key, double def) const {
    const QVariant v = get(section, key);
    return v.isNull() ? def : v.toDouble();
}

bool Config::boolean(const QString& section, const QString& key, bool def) const {
    const QVariant v = get(section, key);
    return v.isNull() ? def : v.toBool();
}

QStringList Config::strList(const QString& section, const QString& key) const {
    const QVariant v = get(section, key);
    QStringList out;
    if (v.canConvert<QVariantList>()) {
        const QVariantList list = v.toList();
        for (const QVariant& item : list)
            out << item.toString();
    }
    return out;
}

bool Config::hasSection(const QString& section) const {
    return m_sections_.contains(section);
}

} // namespace pano
