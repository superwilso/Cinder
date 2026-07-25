//
// Copyright 2015-2017 Sony Corporation
//

/**
 * @brief Source Directスイッチボタン
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
    /**
     * Press時シグナル
     * @param Nothing
     */
    signal pressed()
    /**
     * Release時シグナル
     * @param Nothing
     */
    signal released()

    width: 88
    height: 88

    Rectangle {
        id: background
        anchors.fill: parent
        color: viewstyle.bgcolor.D1
    }

    SCmnSwitchButton {
        id: cmnSwitchButtonForClrAudioAndSrcDirect
        anchors.fill: parent
        checked: base.checked && base.enabled   // enabledがfalse(disableモード)の際、スイッチ表示をOFFにする
        grooveDisabledUrl: "qrc:/switch_se_setting_source_direct_background_disable.png"
        grooveOnUrl: "qrc:/switch_se_setting_source_direct_background_on.png"
        grooveOffUrl: "qrc:/switch_se_setting_source_direct_background_off.png"
        handleDisabledUrl: "qrc:/switch_se_setting_source_direct_thumb_disable.png"
        handleOnUrl: "qrc:/switch_se_setting_source_direct_thumb_on.png"
        handleOffUrl: "qrc:/switch_se_setting_source_direct_thumb_off.png"
        grooveImplicitWidth: 88
        grooveImplicitHeight: 88
        handleImplicitWidth: 40
        handleImplicitHeight: 88
        onClicked: base.clicked()
    }
}
