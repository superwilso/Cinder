//
// Copyright 2015-2017 Sony Corporation
//
import QtQuick 2.3
import "qrc:/parts"

Item {
    id: base

    width: 480
    height: 580

    property int currentIndexYear  : 0
    property int currentIndexMonth : 0
    property int currentIndexDay   : 0
    property int currentIndexHour  : 0
    property int currentIndexMinute: 0
    property int currentIndexAmPm  : 0
    property int minimumYear       : 0
    property alias dateMode: datePicker.dateMode    //日時表示形式　0:YYYY/MM/DD, 1:MM/DD/YYYY, 2:DD/MM/YYYY
    property alias timeMode: timePicker.timeMode    //false: 12時間表示、true: 24時間表示
    property bool interactive: true                 //false: フリック不可、true: フリック可

    signal yearValueChanged(int value)              // 年
    signal monthValueChanged(int value)             // 月
    signal dayValueChanged(int value)               // 日
    signal hourValueChanged(int value)              // 時
    signal minuteValueChanged(int value)            // 分
    signal amPmValueChanged(int value)              // AM/PM

    onCurrentIndexYearChanged:   datePicker.currentIndexYear = base.currentIndexYear
    onCurrentIndexMonthChanged:  datePicker.currentIndexMonth = base.currentIndexMonth
    onCurrentIndexDayChanged:    datePicker.currentIndexDay = base.currentIndexDay
    onCurrentIndexHourChanged:   timePicker.currentIndexHour = base.currentIndexHour
    onCurrentIndexMinuteChanged: timePicker.currentIndexMinute = base.currentIndexMinute
    onCurrentIndexAmPmChanged:   timePicker.currentIndexAmPm = base.currentIndexAmPm
    onMinimumYearChanged:        datePicker.minimumYear = base.minimumYear

    QtObject {
        id: p
        property int lineTopMargin: 34
        property int lineHeight: 1
        property int timePrikerTopMargin: 30
        property int controlHeight: 25
    }

    // 年月日
    SCmnDatePicker {
        id: datePicker
        anchors {top: base.top; horizontalCenter: base.horizontalCenter}
        interactive: base.interactive
        onYearValueChanged:  base.yearValueChanged(value)
        onMonthValueChanged: base.monthValueChanged(value)
        onDayValueChanged:   base.dayValueChanged(value)
    }

    // セパレータ
    Image {
        id: line
        height: p.lineHeight
        source: "qrc:/ic_date_time_setting_area_separetor.png"
        anchors {top: datePicker.bottom; topMargin: p.lineTopMargin - p.controlHeight; horizontalCenter: base.horizontalCenter}
    }

    // 時間
    SCmnTimePicker {
        id: timePicker
        anchors {top: line.bottom; topMargin: p.timePrikerTopMargin - p.controlHeight ; horizontalCenter: base.horizontalCenter}
        interactive: base.interactive
        onHourValueChanged:   base.hourValueChanged(value)
        onMinuteValueChanged: base.minuteValueChanged(value)
        onAmPmValueChanged:   base.amPmValueChanged(value)
    }

}
