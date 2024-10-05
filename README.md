# nom

A CLI nutrition tracker

## Usage

### Foods

```sh
# Search for a food online, and add it to nom's database.
nom food search <term...>

# Edit a food/recipe/journal in your editor.
# If key doesn't exist, it is created.
# This can be used to manually add foods.
nom food edit <key>

# View a food/recipe/journal in the terminal.
nom food show <key>
```

### Recipes

```sh
nom recipe show <key>
nom recipe edit <key>
```

### Journals

```sh
# Add a serving of a food or recipe to today's journal
nom nom <food|recipe> [serving]

# Show all food consumed on a day.
nom journal show [day]

# Edit the journal for the given day in your editor.
nom journal edit [day]
```
