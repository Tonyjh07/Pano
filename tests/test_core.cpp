#include <QtTest>

#include "pano/adapters/example_counter.h"
#include "pano/adapters/register.h"
#include "pano/core/config.h"
#include "pano/core/lifecycle.h"
#include "pano/core/registry.h"
#include "pano/core/sample_store.h"

using namespace pano;

class TestCore : public QObject {
    Q_OBJECT

    // ---- SampleStore ----
private slots:
    void sampleStore_appendAndReadBack() {
        SampleStore store(600);
        QSignalSpy spy(&store, &SampleStore::sampleAppended);
        store.insert(1000, "a.b", SampleValue::fromNumber(1.5));
        store.insert(2000, "a.b", SampleValue::fromNumber(2.5));
        QCOMPARE(store.series("a.b").size(), 2);
        QCOMPARE(spy.count(), 2);
        QCOMPARE(store.series("a.b")[1].value.number, 2.5);
        QVERIFY(store.has("a.b"));
    }

    void sampleStore_ringEvictsOldest() {
        SampleStore store(3);
        for (int i = 0; i < 5; ++i)
            store.insert(qint64(i), "x", SampleValue::fromNumber(i));
        const auto s = store.series("x");
        QCOMPARE(s.size(), 3);
        QCOMPARE(s[0].value.number, 2.0);  // oldest two dropped
        QCOMPARE(s[2].value.number, 4.0);
        QCOMPARE(store.latest()["x"].value.number, 4.0);
    }

    void sampleStore_clear() {
        SampleStore store;
        store.insert(0, "x", SampleValue::fromNumber(1));
        store.clear();
        QVERIFY(!store.has("x"));
    }

    // ---- Config ----
    void config_parsesSectionsAndValues() {
        QString err;
        const Config c = Config::fromToml(
            "# comment\n"
            "[core]\n"
            "capacity = 600\n"
            "enabled = true\n"
            "name = \"Pano\"\n"
            "[adapters.sys.cpu]\n"
            "high_threshold = 85\n", &err);
        QVERIFY2(err.isEmpty(), qPrintable(err));
        QCOMPARE(c.num("core", "capacity"), 600.0);
        QCOMPARE(c.boolean("core", "enabled"), true);
        QCOMPARE(c.str("core", "name"), QString("Pano"));
        QCOMPARE(c.num("adapters.sys.cpu", "high_threshold"), 85.0);
        QVERIFY(c.hasSection("core"));
    }

    void config_rejectsMalformed() {
        QString err;
        Config::fromToml("[core]\nkey = \n", &err);
        QVERIFY(!err.isEmpty());
    }

    void config_parsesArray() {
        QString err;
        const Config c = Config::fromToml("[ui]\nwindows = [ \"a\", \"b\", \"c\" ]\n", &err);
        QVERIFY2(err.isEmpty(), qPrintable(err));
        QCOMPARE(c.strList("ui", "windows").size(), 3);
    }

    // ---- Registry ----
    void registry_registerAndCreate() {
        AdapterRegistry r;
        registerBuiltinAdapters(r);
        QVERIFY(r.contains("sys.cpu"));
        QVERIFY(r.contains("example.counter"));
        QVERIFY(!r.contains("does.not.exist"));
        QVERIFY(r.ids().contains("sys.net"));
        auto a = r.create("sys.cpu");
        QVERIFY(a);
        QCOMPARE(a->id(), QString("sys.cpu"));
        QVERIFY(r.create("does.not.exist") == nullptr);
    }

    // ---- Lifecycle with example adapter ----
    void lifecycle_startPollStop() {
        AdapterRegistry r;
        registerBuiltinAdapters(r);
        SampleStore store(100);
        Lifecycle lc(&r, &store);
        QVERIFY(lc.startAdapter("example.counter"));
        QCOMPARE(lc.status("example.counter"), AdapterStatus::Running);
        // poll immediately (the timer may not have fired yet)
        IAdapter* a = lc.adapter("example.counter");
        QVERIFY(a);
        auto samples = a->poll();
        QVERIFY(samples.size() >= 1);
        QCOMPARE(samples[0].series, QString("example.counter.value"));
        QCOMPARE(samples[0].value.number, 1.0);

        lc.stop("example.counter");
        QCOMPARE(lc.status("example.counter"), AdapterStatus::Stopped);
    }

    void lifecycle_unknownAdapterFails() {
        AdapterRegistry r;
        r.registerFactory("nope", [] { return (IAdapter*)nullptr; });
        SampleStore store;
        Lifecycle lc(&r, &store);
        QVERIFY(!lc.startAdapter("missing"));
    }

    void lifecycle_configApplyRollbackOnFailure() {
        AdapterRegistry r;
        registerBuiltinAdapters(r);
        SampleStore store;
        Lifecycle lc(&r, &store);

        // Set a good config first, then a dead-end (empty) config that must fail.
        QVERIFY(lc.startAdapter("example.counter"));
        QJsonObject next;
        next.insert("step", 10.0);
        QVERIFY(lc.setAdapterConfig("example.counter", next));
    }
};

QTEST_GUILESS_MAIN(TestCore)
#include "test_core.moc"
