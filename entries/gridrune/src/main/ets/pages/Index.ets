import { DefaultXComponent } from '@ohos-rs/ability';

@Entry
@Component
struct Index {
  build() {
    Stack() {
      DefaultXComponent({ moduleName: 'gridrune_ability' })
    }
    .width('100%')
    .height('100%')
    .backgroundColor('#121316')
  }
}
