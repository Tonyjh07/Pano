#pragma once

#include <QApplication>
#include <QString>

namespace pano {

// Plain entry-point helper so main.cpp stays tiny and testable.
int runApp(int argc, char** argv);

} // namespace pano
