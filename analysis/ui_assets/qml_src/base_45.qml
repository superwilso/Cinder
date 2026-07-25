//
// Copyright 2015-2017 Sony Corporation
//

/**
 * @brief Fm Radioスイッチボタン
　*/
import QtQuick 2.0
import "qrc:/parts"
import "qrc:/"

Item {
    id: base

    /** type bool スイッチボタンのON/OFFの状態 */
    property bool checked: false
    /**
     * Click時シグナル
     * @param Nothing
     */
    signal clicked()

    SCmnSwitchButton {
        id: cmnSwitchButtonForFmRadio
        anchors.fill: parent
        checked: base.checked && base.enabled   // enabledがfalse(disableモード)の際、スイッチ表示をOFFにする
        grooveDisabledUrl:"qrc:/assets/images/fm_radio/switch_fm_power_disable.png" 
        grooveOnUrl: "qrc:/assets/images/fm_radio/switch_fm_power_on.png"
        grooveOffUrl: "qrc:/assets/images/fm_radio/switch_fm_power_off.png"
        handleDisabledUrl: "qrc:/assets/images/fm_radio/switch_fm_power_thumb_disable.png"
        handleOnUrl: "qrc:/assets/images/fm_radio/switch_fm_power_thumb_on.png"
        handleOffUrl: "qrc:/assets/images/fm_radio/switch_fm_power_thumb_off.png"
        grooveImplicitWidth: 88
        grooveImplicitHeight: 88
        handleImplicitWidth: 40
        handleImplicitHeight: 88
        onClicked: base.clicked()
    }
}
