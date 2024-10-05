# nosh

A CLI nutrition tracker

## Usage

### Foods

```sh
# Search for a food online, and add it to nosh's database.
nosh food search <term...>

# Edit a food/recipe/journal in your editor.
# If key doesn't exist, it is created.
# This can be used to manually add foods.
nosh food edit <key>

# View a food/recipe/journal in the terminal.
nosh food show <key>
```

### Recipes

```sh
nosh recipe show <key>
nosh recipe edit <key>
```

### Journals

```sh
# Add a serving of a food or recipe to today's journal
nosh nosh <food|recipe> [serving]

# Show all food consumed on a day.
nosh journal show [day]

# Edit the journal for the given day in your editor.
nosh journal edit [day]
```
