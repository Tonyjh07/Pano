#include <QtTest>

#include "pano/adapters/example_counter.h"
#include "pano/adapters/register.h"
#include "pano/adapters/sys/sys_cpu.h"
#include "pano/adapters/sys/sys_disk.h"
#include "pano/adapters/sys/sys_mem.h"
#include "pano/adapters/sys/sys_net.h"
#include "pano/core/registry.h"
#include "pano/core/sample_store.h"

using namespace pano;

namespace {
struct PushingCtx : AdapterContext {
    SampleStore store;
    void emitValue(const QString& s, const SampleValue& v, qint64 ts) override { store.insert(ts, s, v); }
    qint64 nowMs() const override { return 42; }
};
}

class TestAdapters : public QObject {
    Q_OBJECT
private slots:
    void exampleCounter_producesSeriesAndHonorsStep() {
        ExampleCounterAdapter a;
        PushingCtx ctx;
        QVERIFY(a.start(&ctx));
        QJsonObject cfg; cfg.insert("step", 7.0);
        a.setConfig(cfg);
        auto s = a.poll();
        QCOMPARE(static_cast<int>(s.size()), 2);
        QCOMPARE(s[0].series, QString("example.counter.value"));
        QCOMPARE(s[0].value.number, 7.0);
        QCOMPARE(s[1].series, QString("example.counter.sine"));
        QVERIFY(qAbs(s[1].value.number) <= 1.0);
        a.stop();
    }

    void cpu_usagePercent() {
        // 0% idle, full busy
        QCOMPARE(SysCpuAdapter::usagePercent(0, 100), 100.0);
        // 30% idle among 100 total
        QCOMPARE(SysCpuAdapter::usagePercent(30, 100), 70.0);
        QCOMPARE(SysCpuAdapter::usagePercent(100, 100), 0.0);
        // guards
        QCOMPARE(SysCpuAdapter::usagePercent(0, 0), 0.0);
    }

    void cpu_pollEmitsUsageAfterWarmup() {
        SysCpuAdapter a;
        PushingCtx ctx;
        QVERIFY(a.start(&ctx));
        QTest::qWait(120);
        QVector<Sample> samples = a.poll();
        QVERIFY(!samples.isEmpty());
        QCOMPARE(samples.first().series, QString("sys.cpu.usage"));
        QVERIFY(samples.first().value.isNumber());
        QVERIFY(samples.first().value.number >= 0.0 && samples.first().value.number <= 100.0);
        a.stop();
    }

    void mem_pollEmitsThreeSeries() {
        SysMemAdapter a;
        PushingCtx ctx;
        QVERIFY(a.start(&ctx));
        auto samples = a.poll();
        QCOMPARE(static_cast<int>(samples.size()), 3);
        QCOMPARE(samples[0].series, QString("sys.mem.used_percent"));
        QCOMPARE(samples[1].series, QString("sys.mem.used_gb"));
        QCOMPARE(samples[2].series, QString("sys.mem.total_gb"));
        QVERIFY(samples[0].value.number >= 0.0);
        a.stop();
    }

    void disk_pollEmitsUsageSeries() {
        SysDiskAdapter a;
        PushingCtx ctx;
        QVERIFY(a.start(&ctx));
        auto samples = a.poll();
        QCOMPARE(static_cast<int>(samples.size()), 3);
        QCOMPARE(samples[0].series, QString("sys.disk.used_percent"));
        a.stop();
    }

    void net_pollEmitsRatesAndUtilization() {
        SysNetAdapter a;
        PushingCtx ctx;
        QVERIFY(a.start(&ctx));
        QTest::qWait(60);
        auto samples = a.poll();
        QCOMPARE(static_cast<int>(samples.size()), 4);
        QCOMPARE(samples[0].series, QString("sys.net.rate_bps"));
        QCOMPARE(samples[3].series, QString("sys.net.utilization"));
        QVERIFY(samples[3].value.number >= 0.0 && samples[3].value.number <= 100.0);
        a.stop();
    }

    void registration_providesSysAdapters() {
        AdapterRegistry r;
        registerBuiltinAdapters(r);
        QVERIFY(r.contains("sys.cpu"));
        QVERIFY(r.contains("sys.mem"));
        QVERIFY(r.contains("sys.disk"));
        QVERIFY(r.contains("sys.net"));
    }
};

QTEST_GUILESS_MAIN(TestAdapters)
#include "test_adapters.moc"
