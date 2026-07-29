//
// Copyright 2015-2016 Sony Corporation
//

import QtQuick 2.3
import QtQuick.Window 2.2
import "../components"

Item {
    id: basicWindow

    property var navigationBar

    InitialSettingWizardHeaderArea { // 28
        id: header
        anchors.top: parent.top
    }

    NavigationBar { // 56
        id: footer
        anchors.bottom: parent.bottom
        properties: basicWindow.navigationBar
    }
}
