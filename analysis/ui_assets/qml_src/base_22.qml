//
// Copyright 2015-2016 Sony Corporation
//
import QtQuick 2.3
import QtQuick.Controls 1.2

Item {
    id: base
    property var model: dmpDialog.list_model
    signal selected(var item_id)
    signal accepted()
    signal closed();

    height: dmpDialog.height
    width: dmpDialog.width

    Component.onCompleted: {
        tempModel.createData()
    }

    Connections {
        target: dmpDialog
        onOptionMenuBack: {
            decrementLevel()
        }
    }

    function decrementLevel() {
        base.closed()
    }

    ListModel {
      id: tempModel

      function createData() {
        // メニュー1階層目の絞込みを実施
        tempModel.clear()
        if (base.model !== null) {
            var i
            var list_count = base.model.count()
            for (i = 0; i < list_count; i++) {
                if (base.model.level(i) === 1) {
                    tempModel.append({list_data: base.model.listData(i),
                                      level: base.model.level(i),
                                      groupL1: base.model.parentIndex(i),
                                      index: base.model.listIndex(i),
                                      type: base.model.type(i),
                                      coverIcon:base.model.coverIcon(i)})
                }
            }
        }
      }
    }

    Rectangle {
        id: opacityArea
        anchors { fill: parent; bottomMargin: p.bottomSpace; }  // NavigationBarの高さ分考慮
        opacity: 0.0
        MouseArea {
            anchors.fill: parent
            onClicked: {
                console.log("opacity area click")
                base.closed()
            }
        }
    }

    Item {
        id: menuArea
        height: levelOneMenu.height
        width: levelOneMenu.width
        anchors {
            bottom: base.bottom; right: base.right;
            bottomMargin: p.menuBottomSpace; rightMargin: p.menu_right_margin;
        }
        clip: true
        OptionMenuList {
            id: levelOneMenu
            x: 0
            height: (p.visibleAreaCellCount < tempModel.count )? p.menu_max_height : (levelOneMenu.cell_height * tempModel.count) + (p.menu_separator_height * (tempModel.count - 1)) + p.menu_vertical_margin
            anchors { bottom: parent.bottom; right: parent.right; }
            model: tempModel
            visible_area_cell_count: p.visibleAreaCellCount
            onClicked: {
                base.selected(index)
                base.accepted()
            }
        }
    }

    QtObject {
        id: p
        property int visibleAreaCellCount: 6
        property int bottomSpace: 56
        property int menuBottomSpace: 106
        property int menu_right_margin: 24
        property int menu_vertical_margin: 5 * 2
        property int menu_separator_height: 1
        property int menu_max_height: 581
    }
}
