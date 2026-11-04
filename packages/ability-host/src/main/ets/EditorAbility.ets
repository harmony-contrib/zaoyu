import { KeyboardAvoidMode } from '@kit.ArkUI';
import { LazyPlugin, NativeAbility, BridgePluginHookContext, BridgeLifecycleEvent } from '@ohos-rs/ability';
import { WindowPlugin } from '@ohos-rs/ability-plugin-window';
import { ClipboardPlugin } from '@ohos-rs/ability-plugin-clipboard';
import { UrlPlugin } from '@ohos-rs/ability-plugin-url';
import { FilesPlugin } from '@ohos-rs/ability-plugin-files';
import { AppControlPlugin } from '@ohos-rs/ability-plugin-app-control';
import { PermissionPlugin } from '@ohos-rs/ability-plugin-permission';
import { MenuPlugin } from '@ohos-rs/ability-plugin-menu';
import { NotificationPlugin } from '@ohos-rs/ability-plugin-notification';
import { ProcessPlugin } from '@ohos-rs/ability-plugin-process';
import { SystemStatePlugin } from '@ohos-rs/ability-plugin-system-state';
import { AccessibilityPlugin } from '@ohos-rs/ability-plugin-accessibility';

class EditorWindowPlugin extends WindowPlugin {
  override async onLifecycle(event: BridgeLifecycleEvent, context: BridgePluginHookContext): Promise<void> {
    await super.onLifecycle(event, context);
    if (event.kind === 'ui-context-ready') {
      context.getUIContext().setKeyboardAvoidMode(KeyboardAvoidMode.RESIZE);
    }
  }
}

export function editorPlugins(scheme: string): LazyPlugin[] {
  return [
    new LazyPlugin(() => new MenuPlugin()),
    new LazyPlugin(() => new NotificationPlugin({ scheme: scheme })),
    new LazyPlugin(() => new ProcessPlugin()),
    new LazyPlugin(() => new SystemStatePlugin()),
    new LazyPlugin(() => new AccessibilityPlugin()),
    new LazyPlugin(() => new PermissionPlugin()),
    new LazyPlugin(() => new EditorWindowPlugin()),
    new LazyPlugin(() => new ClipboardPlugin()),
    new LazyPlugin(() => new UrlPlugin()),
    new LazyPlugin(() => new FilesPlugin(false)),
    new LazyPlugin(() => new AppControlPlugin())
  ];
}

export class EditorAbility extends NativeAbility {
  public defaultPage: boolean = true;
}
