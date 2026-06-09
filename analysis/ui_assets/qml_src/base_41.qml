//
// Copyright 2018 Sony Corporation
//

import QtQuick 2.3
import QtQuick.Controls 1.2
import QtQuick.Controls.Styles 1.2
import "qrc:/components"
import "qrc:/parts"
import "qrc:/"

ScreenBase
{
  id: base

  property string title_string
  property var list_data
  property Component delegate_component

  ScreenTitleArea
  {
    id: title_area
    titleText: title_string
    visibleMusicLibraryTopicon: false
    fontWeight: Font.Light
  }

  ListView
  {
    id: list
    anchors
    {
      top: title_area.bottom
      bottom: base.bottom
      left: base.left
      right: base.right
    }
    boundsBehavior: Flickable.StopAtBounds
    flickableDirection: Flickable.VerticalFlick
    model: list_data
    delegate: delegate_component
  }
}
