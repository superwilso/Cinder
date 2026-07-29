//
// Copyright 2015-2016 Sony Corporation
//

import QtQuick 2.3
import QtQuick.Controls 1.2
import "qrc:/components"
import "qrc:/parts"

Item {
    id: regulationInfo

    property bool isValidImagePath
    property bool isEmptyImage
    property string regulationInfoImagePath

    property int flickable_area_height: 612
    property int flickable_area_width: 480

    Rectangle{
      anchors.fill: parent
      color: viewstyle.bgcolor.D1
    }

    ScreenTitleArea {
      id: regulationInfoTitleArea
      titleText: qsTr("010019") + viewstyle.empty_string
      visibleMusicLibraryTopicon: false
    }

    Flickable {
      id: regulationInfoFlickableArea
      anchors {top:regulationInfoTitleArea.bottom; left: parent.left;}
      width: flickable_area_width
      height: flickable_area_height
      contentHeight: regulationInfoItems.height
      clip: true
      boundsBehavior: Flickable.StopAtBounds
      visible:contentHeight>0? true:false

      Item {
        id: regulationInfoItems
        anchors { top: parent.top; right: parent.right; left: parent.left;}
        height: childrenRect.height

        SCmnLabel{
          id: regulationInfoErrorText
          anchors {top:parent.top; left: parent.left;}
          height: flickable_area_height
          width: flickable_area_width
          color:"red"
          font.pixelSize: 150
          text: qsTr("ERROR!!!!") + viewstyle.empty_string
          visible: isEmptyImage
        }

        Image{
          id: regulationInfoImage
          anchors {top:parent.top; left: parent.left;}
          height:sourceSize.height
          width:sourceSize.width
          visible: isValidImagePath
          source: "file:" + regulationInfoImagePath
        }
      }
    }

    SCmnScrollIndicator {
      id: regulationInfoScroll
      anchors {top:regulationInfoFlickableArea.top; bottom: regulationInfoFlickableArea.bottom; right: regulationInfoFlickableArea.right;}
      flickable: regulationInfoFlickableArea
    }
}
