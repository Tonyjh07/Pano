#include <QtTest>

#include "pano/ui/gauge_geometry.h"
#include "pano/ui/gauge_widget.h"

using namespace pano;

class TestUi : public QObject {
    Q_OBJECT
private slots:
    void gaugeGeometry_angleConventions() {
        QCOMPARE(GaugeGeometry::angleFor(0.0, 0.0, 100.0), -120.0);   // 8 o'clock
        QCOMPARE(GaugeGeometry::angleFor(50.0, 0.0, 100.0), 0.0);      // 12 o'clock
        QCOMPARE(GaugeGeometry::angleFor(100.0, 0.0, 100.0), 120.0);   // 4 o'clock
        QCOMPARE(GaugeGeometry::angleFor(75.0, 0.0, 100.0), 60.0);
    }

    void gaugeGeometry_clamps() {
        QCOMPARE(GaugeGeometry::angleFor(-10.0, 0.0, 100.0), -120.0);
        QCOMPARE(GaugeGeometry::angleFor(999.0, 0.0, 100.0), 120.0);
        QCOMPARE(GaugeGeometry::angleFor(0.0, 0.0, 0.0), -120.0);
    }

    void gaugeGeometry_fractionAndThreshold() {
        QCOMPARE(GaugeGeometry::fraction(50.0, 0.0, 100.0), 0.5);
        QVERIFY(GaugeGeometry::overThreshold(81.0, 80.0));
        QVERIFY(!GaugeGeometry::overThreshold(79.9, 80.0));
    }

    void gaugeWidget_rendersWithoutCrash() {
        GaugeWidget w;
        w.setConfig(GaugeWidget::Config{0, 100, 80, "%", "CPU"});
        w.setValue(73.5);
        w.resize(200, 168);
        w.show();
        const QPixmap pm = w.grab();
        QVERIFY(!pm.isNull());
        QCOMPARE(pm.width(), 200);
        QCOMPARE(w.config().maxValue, 100.0);
    }
};

QTEST_MAIN(TestUi)
#include "test_ui.moc"
