import { EditorAbility, editorPlugins } from '@richerfu/ability-host';

export default class EntryAbility extends EditorAbility {
  public moduleName: string = 'deckune_ability';
  public bridgePlugins = editorPlugins('deckune');
}
