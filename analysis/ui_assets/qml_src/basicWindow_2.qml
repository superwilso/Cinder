//
// Copyright 2015-2016 Sony Corporation
//
import QtQuick 2.3
import QtQuick.Window 2.2
import "../components"

Item {
    id: basicWindow
    visible: true

    property var headerArea
    property var navigationBar

    HeaderArea { // 44
        id: header
        anchors.top: parent.top

        properties: basicWindow.headerArea
    }

    NavigationBar { // 56
        id: footer
        anchors.bottom: parent.bottom
        properties: basicWindow.navigationBar
    }
}
