//
// Copyright 2015-2016 Sony Corporation
//
import QtQuick 2.0

Item {
    property int nowIndex: 0
    property int markcount: 14
    property var disabledIndexList: null

    function getIndicatorImagePath(index)
    {
        if ((disabledIndexList !== null) && (disabledIndexList.indexOf(index) >= 0))
        {
            return "qrc:/assets/images/sensme_player/page_indicator_sensme_channel_disable.png"
        }
        else
        {
            return (index === nowIndex)
                    ? "qrc:/assets/images/sensme_player/page_indicator_sensme_channel_current.png"
                    : "qrc:/assets/images/sensme_player/page_indicator_sensme_channel_normal.png"
        }
    }

    Row {
        anchors.centerIn: parent

        Repeater {
            model: markcount

            Image {
              width: 18; height: 18
              source: getIndicatorImagePath(index)
            }
        }
    }
}
