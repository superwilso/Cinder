//
// Copyright 2015-2016 Sony Corporation
//

import QtQuick 2.0
import QtQuick.Controls 1.2
import "qrc:/components"
import "qrc:/components/menu"
import "qrc:/framework"
import "qrc:/parts"
Item
{
    property var properties
    objectName: properties["objectName"]
    property var msgVisible: properties["msgVisible"]

    width:400
    height:116
    Image {
        id: popup
        anchors.fill:parent
        visible:msgVisible === undefined ? false : msgVisible
        source: "qrc:/assets/images/popup_area_center_4r.png"
        SCmnLabel {
            anchors.centerIn: parent
            verticalAlignment: Text.AlignVCenter
            width: 352
            height: 96
            lineHeightMode: Text.FixedHeight
            lineHeight: 33
            text: qsTr("010113") + viewstyle.empty_string
            wrapMode: Text.WordWrap
            font.pixelSize: viewstyle.textsize.M
            font.weight: Font.Light
            color: viewstyle.textcolor.L1
        }
    }
}
