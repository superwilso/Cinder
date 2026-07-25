//
// Copyright 2015-2016 Sony Corporation
//
import QtQuick 2.0

Rectangle {
    width: 480
    height: 88
    color: "black"

    property alias function_icon: function_icon

    FunctionIcon {
        id: function_icon
        anchors {
            top: parent.top
            left: parent.left; leftMargin: function_icon_info["function_icon_area_margin"]
        }
    }
}
