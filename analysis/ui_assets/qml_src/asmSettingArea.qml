//
// Copyright 2017 Sony Corporation
//

import QtQuick 2.3
import QtQuick.Controls 1.2
import QtQuick.Controls.Styles 1.2
import "qrc:/components"
import "qrc:/parts"

Rectangle {
  id: asmSettingArea
  width: 480
  color: viewstyle.bgcolor.D1

  property bool isAsmOn
  property int asmLevelValue
  property var currentAsmModeName
  property var asmModeList
  property bool isAvailableAsmModeSetting

  signal changeAsmOnOffState()
  signal selectModeItem(int index)
  signal changeLevelValue(int value)

  ScreenTitleArea {
    id: asmSettingTitleArea
    z: 11
    titleText: qsTr("100014") + viewstyle.empty_string
    visibleMusicLibraryTopicon: false
    fontWeight: Font.Light
  }
  Column {
    id: column
    y: asmSettingTitleArea.height
    height: childrenRect.height

    // ON/OFFスイッチ
    SettingItemSwitchButton {
      checked: isAsmOn
      onClicked: changeAsmOnOffState()
    }

    // モード選択
    SCmnLabel {
      id: modeSettingTitle
      visible: isAvailableAsmModeSetting
      width: 440
      height: 84
      x: 20
      text: qsTr("100016") + viewstyle.empty_string
      verticalAlignment: Text.AlignVCenter
      font.pixelSize: viewstyle.textsize.L
      font.weight: Font.Light
      enabled: isAsmOn
    }
    Image {
      visible: isAvailableAsmModeSetting
      source: "qrc:/assets/images/separator_sub2.png"
    }
    SettingItemPullDownList {
      visible: isAvailableAsmModeSetting
      dialogTitle: qsTr("100016") + viewstyle.empty_string
      listData: asmModeList
      currentText: currentAsmModeName.toString()
      onSelected: selectModeItem(item_id)
      pulldownEnabled: isAsmOn
    }

    // 外音取り込みレベル
    SCmnLabel {
      id: asmLevelSettingTitle
      width: 440
      height: 84
      x: 20
      text: qsTr("100015") + viewstyle.empty_string
      verticalAlignment: Text.AlignVCenter
      font.pixelSize: viewstyle.textsize.L
      font.weight: Font.Light
      enabled: isAsmOn
    }
    Item {
      id: asmLevelSettingSliderArea
      width: 480
      height: 84
      Image {
        source: isAsmOn ? "qrc:/assets/images/setting_top/slider_setting_amb_guide_base.png"
                        : "qrc:/assets/images/setting_top/slider_setting_amb_guide_base_disable.png"
      }
      SCmnSlider {
        id: asmLevelSettingSlider
        width: 366
        height: 84
        x: 38
        enabled: isAsmOn
        minValue: -15
        maxValue: 15
        sliderStep: 1
        value: asmLevelValue
        onValueChanged: {
          changeLevelValue(value)
        }
        slider_style: SliderStyle {
          groove: Item {
            width: 366
            height: 84
          }
          handle: Image {
            width: 32
            height: 32
            y: -(asmLevelSettingSlider.height / 2 - height / 2)
            source: isAsmOn ? "qrc:/assets/images/setting_top/slider_setting_nc_thumb.png"
                            : "qrc:/assets/images/setting_top/slider_setting_nc_thumb_disable.png"
          }
        }
      }
      SCmnMonospaceLabel {
        id: asmLevelValueLabel
        width: 60
        x: 420
        z: 5
        text: (asmLevelValue <= 0) ? asmLevelValue.toString() : ("+" + asmLevelValue.toString())
        verticalAlignment: Text.AlignVCenter
        horizontalAlignment: Text.AlignHCenter
        font.pixelSize: 28
        font.weight: Font.Light
        enabled: isAsmOn
      }
    }
  }
  DisclaimerGuide {
    anchors {
      top: column.bottom
      left: parent.left
      bottom: parent.bottom
      right: parent.right
    }
    guideText: qsTr("100035") + viewstyle.empty_string
  }
}
