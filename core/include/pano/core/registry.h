#pragma once

#include "pano/core/i_adapter.h"

#include <QHash>
#include <QString>
#include <QVector>

#include <functional>
#include <memory>

namespace pano {

// Compile-time adapter registry. Mirrors the Rust `Registry`: adapters register
// a factory keyed by id; the lifecycle instantiates them on demand.
class AdapterRegistry {
public:
    using Factory = std::function<IAdapter*()>;

    void registerFactory(const QString& id, Factory f) { m_factories_.insert(id, std::move(f)); }
    bool contains(const QString& id) const { return m_factories_.contains(id); }
    QVector<QString> ids() const { return m_factories_.keys(); }

    std::unique_ptr<IAdapter> create(const QString& id) const {
        auto it = m_factories_.constFind(id);
        if (it == m_factories_.constEnd())
            return nullptr;
        return std::unique_ptr<IAdapter>((*it)());
    }

private:
    QHash<QString, Factory> m_factories_;
};

} // namespace pano
