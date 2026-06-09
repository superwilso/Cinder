//
// Copyright 2015-2016 Sony Corporation
//

import QtQuick 2.3
import "../components"

Item {
  id: simpleWindow

  visible: true

  property var headerArea
  property string playTargetName
  property string playIndexAndTotalCount

  HeaderArea { // 44
      id: header
      anchors.top: parent.top

      properties: simpleWindow.headerArea
  }
  MusicPlayingHeader { // 36
      id: musicPlayingHeader
      anchors.top: header.bottom
      anchors.left: parent.left
      playTargetName: base.playTargetName
      playIndexAndTotalCount: base.playIndexAndTotalCount
  }
}
