//
// Copyright 2015-2017 Sony Corporation
//

/**
 * @brief 88 * 88 pixelラジオボタン
 * 複数のラジオボタンを同一グループにするためには、ExclusiveGroupを設定する必要があります。
　*/
import QtQuick 2.3
import QtQuick.Controls 1.2
import "qrc:/"

Item {
    id: base
    /** type ExclusiveGroup ラジオボタングループ */
    property ExclusiveGroup exclusiveGroup: null
    /** type bool ラジオボタンの選択状態 */
    property bool checked
    /** type bool テキストエリアのタッチ状態 */
    property bool textAreaPressed: false
    /** isEnablePressAndHold 長押しイベント有効・無効プロパティ。isEnablePressAndHoldはfalseである場合、長押しイベントを拒否する。 */
    property bool isEnablePressAndHold: false
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
    /**
     * Press and hold時シグナル
     * @param Nothing
     */
    signal pressAndHold()

    // The width of the radio button
    width: 88
    // The height of the radio button
    height: 88

    QtObject {
        id: p
        property url btn_radio_off_disable: "qrc:/btn_radio_off_disable_88.png"
        property url btn_radio_off_normal: "qrc:/btn_radio_off_normal_88.png"
        property url btn_radio_off_pressed: "qrc:/btn_radio_off_pressed_88.png"
        property url btn_radio_on_disable: "qrc:/btn_radio_on_disable_88.png"
        property url btn_radio_on_normal: "qrc:/btn_radio_on_normal_88.png"
        property url btn_radio_on_pressed: "qrc:/btn_radio_on_pressed_88.png"
    }

    SCmnRadioButton {
        id: radio
        width: base.width
        height: base.height
        enabled: base.enabled
        isEnablePressAndHold: base.isEnablePressAndHold
        exclusiveGroup: base.exclusiveGroup
        checked: base.checked
        textAreaPressed: base.textAreaPressed
        btn_radio_off_disable: p.btn_radio_off_disable
        btn_radio_off_normal: p.btn_radio_off_normal
        btn_radio_off_pressed: p.btn_radio_off_pressed
        btn_radio_on_disable: p.btn_radio_on_disable
        btn_radio_on_normal: p.btn_radio_on_normal
        btn_radio_on_pressed: p.btn_radio_on_pressed
        onClicked: base.clicked()
        onPressed: base.pressed()
        onReleased: base.released()
        onPressAndHold: base.pressAndHold()
    }
}

