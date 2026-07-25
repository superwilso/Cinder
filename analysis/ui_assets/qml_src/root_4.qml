//
// Copyright 2015-2016 Sony Corporation
//
import QtQuick 2.3
import QtQuick.Window 2.2
import "../components"

Item {
    id: root
    visible: true

    property var navigationBar
    property var headerArea

    HeaderArea { // 44
        id: header
        anchors.top: parent.top

        properties: headerArea
    }

    OperationArea { // 120
        id: operation
        anchors.bottom: footer.top
    }

    NavigationBar { // 56
        id: footer
        anchors.bottom: parent.bottom

        properties: navigationBar
    }
}
