//
// Copyright 2015-2017 Sony Corporation
//
import QtQuick 2.3
import QtQuick.Controls 1.2
import "qrc:/components"
import "qrc:/parts"
import "qrc:/"
ScreenBase {
    id:dacSettingScreenBase

    /**
     * 接続機器から充電するモードを切り替えた際に発信するシグナル
     */
    signal changeChargeMode()

    /** type:bool 接続機器から充電するモードかどうか */
    property bool is_charge_mode

    Item {
        id: msg
        /** USB DAC設定 */
        property string screenTitle                 : qsTr("110000") + viewstyle.empty_string
        /** 接続機器から充電する */
        property string chargeMode               : qsTr("200044") + viewstyle.empty_string
    }

    ScreenTitleArea {
        id: titleArea
        titleText: msg.screenTitle
        visibleMusicLibraryTopicon: false
    }

    Item {
        id: dacConnectSettingItem
        anchors.top:titleArea.bottom

        SettingItemCheckbox {
            title: msg.chargeMode
            checked: is_charge_mode
            onClicked: changeChargeMode()
        }
    }
}

