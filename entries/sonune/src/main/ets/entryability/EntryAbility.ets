import { EditorAbility, editorPlugins } from '@richerfu/ability-host';

export default class EntryAbility extends EditorAbility {
  public moduleName: string = 'sonune_ability';
  public bridgePlugins = editorPlugins('sonune');
}
