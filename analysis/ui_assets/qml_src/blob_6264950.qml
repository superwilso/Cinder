//
// Copyright 2015-2016 Sony Corporation
//
import QtQuick 2.0

Item {
    height: 20
    property int nowIndex: 0
    property int markcount: 2
    visible: markcount >= 2
    property var disabledIndexList: null

    function getIndicatorImagePath(index)
    {
        if ((disabledIndexList !== null) && (disabledIndexList.indexOf(index) >= 0))
        {
            return "file:/opt2/appresouce/page_indicator_disable.png"
        }
        else
        {
            return (index === nowIndex)
                    ? "file:/opt2/appresouce/page_indicator_focus.png"
                    : "file:/opt2/appresouce/page_indicator_normal.png"
        }
    }

    Row {
        anchors.centerIn: parent

        Repeater {
            model: markcount

            Image {
              width: 20; height: 20
              source: getIndicatorImagePath(index)
            }
        }
    }
}
