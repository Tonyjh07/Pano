#pragma once

#include "pano/core/i_adapter.h"

#include <QJsonObject>
#include <QVector>

namespace pano {

// Small shared helpers for adapters.
namespace util {

// Apply known numeric config fields with defaults (numeric schema fields).
inline QJsonObject numericConfig(const QVector<ConfigField>& schema) {
    QJsonObject o;
    for (const ConfigField& f : schema)
        o.insert(f.key, f.defVal);
    return o;
}

} // namespace util

} // namespace pano
