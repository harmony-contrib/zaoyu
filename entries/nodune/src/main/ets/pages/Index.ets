import { common } from '@kit.AbilityKit';
import { AppItem, APPS } from '../catalog';

@Entry
@Component
struct Index {
  @State message: string = '';

  private openEditor(): void {
    const context = this.getUIContext().getHostContext() as common.UIAbilityContext;
    context.startAbility({
      bundleName: 'com.richerfu.vecrune',
      moduleName: 'vecrune',
      abilityName: 'EntryAbility'
    }).catch(() => {
      this.message = '请先安装 Vecrune，再从这里打开。';
    });
  }

  build() {
    Column({ space: 16 }) {
      Image($r('app.media.app_icon')).width(80).height(80).borderRadius(20)
      Text('Nodune').fontSize(32).fontWeight(FontWeight.Bold)
      Text('造隅工坊').fontSize(18).fontColor('#53616B')
      Text('你的创作工具，从这里开始。').fontSize(15).fontColor('#53616B')
      Button('打开 Vecrune · 造隅矢绘')
        .onClick(() => this.openEditor())
      if (this.message.length > 0) {
        Text(this.message).fontSize(14).fontColor('#985423')
      }
      List({ space: 10 }) {
        ForEach(APPS, (app: AppItem) => {
          ListItem() {
            Row({ space: 14 }) {
              Column({ space: 4 }) {
                Text(app.english).fontSize(18).fontWeight(FontWeight.Medium)
                Text(app.chinese).fontSize(14).fontColor('#53616B')
              }.alignItems(HorizontalAlign.Start)
              Blank()
              Text(app.status).fontSize(12).fontColor('#53616B')
            }.width('100%').padding(16).backgroundColor(Color.White).borderRadius(16)
          }
        }, (app: AppItem) => app.id)
      }.layoutWeight(1).width('100%')
    }
    .padding({ top: 32, left: 24, right: 24, bottom: 24 })
    .width('100%').height('100%').backgroundColor('#F3F6F7')
  }
}
