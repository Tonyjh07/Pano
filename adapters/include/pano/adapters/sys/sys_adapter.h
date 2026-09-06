#pragma once

#include "pano/core/i_adapter.h"
#include "pano/adapters/util.h"

#include <QJsonObject>

namespace pano {

// Common base for system monitoring adapters (share capability set + config
// storage). Concrete adapters live in separate files per platform.
class SysAdapter : public IAdapter {
    Q_OBJECT
public:
    explicit SysAdapter(QObject* parent = nullptr) : IAdapter(parent) {}

    QVector<Capability> capabilities() const override {
        return { Capability::SystemInfo, Capability::TimeSeries };
    }
    void setConfig(const QJsonObject& cfg) override { config_ = cfg; }
    QJsonObject config() const override { return config_; }

protected:
    QJsonObject config_;
};

} // namespace pano
