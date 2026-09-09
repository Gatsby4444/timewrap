package app.timewrap.ui

import androidx.compose.foundation.background
import androidx.compose.foundation.border
import androidx.compose.foundation.clickable
import androidx.compose.foundation.layout.Arrangement
import androidx.compose.foundation.layout.Box
import androidx.compose.foundation.layout.Column
import androidx.compose.foundation.layout.PaddingValues
import androidx.compose.foundation.layout.Row
import androidx.compose.foundation.layout.Spacer
import androidx.compose.foundation.layout.fillMaxSize
import androidx.compose.foundation.layout.fillMaxWidth
import androidx.compose.foundation.layout.height
import androidx.compose.foundation.layout.padding
import androidx.compose.foundation.layout.size
import androidx.compose.foundation.layout.width
import androidx.compose.foundation.lazy.LazyColumn
import androidx.compose.foundation.lazy.items
import androidx.compose.foundation.rememberScrollState
import androidx.compose.foundation.shape.CircleShape
import androidx.compose.foundation.verticalScroll
import androidx.compose.material.icons.Icons
import androidx.compose.material.icons.automirrored.outlined.ArrowBack
import androidx.compose.material3.AlertDialog
import androidx.compose.material3.Card
import androidx.compose.material3.CardDefaults
import androidx.compose.material3.ExperimentalMaterial3Api
import androidx.compose.material3.FilterChip
import androidx.compose.material3.Icon
import androidx.compose.material3.IconButton
import androidx.compose.material3.MaterialTheme
import androidx.compose.material3.OutlinedButton
import androidx.compose.material3.OutlinedTextField
import androidx.compose.material3.Scaffold
import androidx.compose.material3.Switch
import androidx.compose.material3.Text
import androidx.compose.material3.TextButton
import androidx.compose.material3.TopAppBar
import androidx.compose.runtime.Composable
import androidx.compose.runtime.LaunchedEffect
import androidx.compose.runtime.getValue
import androidx.compose.runtime.mutableStateOf
import androidx.compose.runtime.remember
import androidx.compose.runtime.setValue
import androidx.compose.ui.Alignment
import androidx.compose.ui.Modifier
import androidx.compose.ui.draw.clip
import androidx.compose.ui.graphics.Color
import androidx.compose.ui.text.font.FontWeight
import androidx.compose.ui.unit.dp
import app.timewrap.UiState
import app.timewrap.core.Category
import app.timewrap.core.Rule
import app.timewrap.core.RuleField
import app.timewrap.core.RuleMatch
import app.timewrap.core.RuleSuggestion

/**
 * L'écran des règles visuelles.
 *
 * Le principe qu'il met en scène : on ne colorie pas des séances, on décrit ce
 * qui les distingue. Le cœur propose d'abord ce qu'il a repéré dans les
 * intitulés importés — c'est la voie rapide ; les règles écrites à la main
 * restent là pour les cas que la déduction rate.
 */
@OptIn(ExperimentalMaterial3Api::class)
@Composable
fun RulesScreen(
    state: UiState,
    onLoad: () -> Unit,
    onAccept: (RuleSuggestion, String, String) -> Unit,
    onSaveRule: (Rule) -> Unit,
    onDeleteRule: (String) -> Unit,
    onCreateCategory: (String, String) -> Unit,
    onUpdateCategory: (String, String, String, UInt) -> Unit,
    onDeleteCategory: (String) -> Unit,
    onBack: () -> Unit,
) {
    var editingRule by remember { mutableStateOf<Rule?>(null) }
    var accepting by remember { mutableStateOf<RuleSuggestion?>(null) }
    var editingCategory by remember { mutableStateOf<Category?>(null) }
    var creatingCategory by remember { mutableStateOf(false) }

    LaunchedEffect(Unit) { onLoad() }

    Scaffold(
        topBar = {
            TopAppBar(
                title = { Text("Règles visuelles") },
                navigationIcon = {
                    IconButton(onClick = onBack) {
                        Icon(Icons.AutoMirrored.Outlined.ArrowBack, contentDescription = "Retour")
                    }
                },
            )
        },
    ) { padding ->
        LazyColumn(
            modifier = Modifier
                .fillMaxSize()
                .padding(padding),
            contentPadding = PaddingValues(16.dp),
            verticalArrangement = Arrangement.spacedBy(10.dp),
        ) {
            if (state.suggestions.isNotEmpty()) {
                item {
                    SectionLabel("Repéré dans vos imports")
                    Text(
                        text = "Un mot qui revient dans plusieurs séances, mais pas dans toutes, " +
                            "découpe l'emploi du temps. Une couleur suffit alors à s'y retrouver.",
                        style = MaterialTheme.typography.bodySmall,
                        color = MaterialTheme.colorScheme.onBackground.copy(alpha = 0.6f),
                    )
                }
                items(state.suggestions, key = { it.pattern }) { suggestion ->
                    SuggestionCard(suggestion) { accepting = suggestion }
                }
            }

            item {
                Spacer(Modifier.height(6.dp))
                SectionLabel("Règles")
            }

            if (state.rules.isEmpty()) {
                item {
                    Text(
                        text = "Aucune règle. Acceptez une suggestion, ou écrivez-en une.",
                        style = MaterialTheme.typography.bodySmall,
                        color = MaterialTheme.colorScheme.onBackground.copy(alpha = 0.6f),
                    )
                }
            }

            items(state.rules, key = { it.id }) { rule ->
                RuleCard(
                    rule = rule,
                    category = state.categories.firstOrNull { it.id == rule.categoryId },
                    onToggle = { onSaveRule(rule.copy(enabled = it)) },
                    onEdit = { editingRule = rule },
                    onDelete = { onDeleteRule(rule.id) },
                )
            }

            item {
                OutlinedButton(
                    onClick = { editingRule = emptyRule() },
                    modifier = Modifier.fillMaxWidth(),
                ) { Text("Nouvelle règle") }
            }

            item {
                Spacer(Modifier.height(6.dp))
                SectionLabel("Catégories")
            }

            items(state.categories, key = { it.id }) { category ->
                CategoryCard(
                    category = category,
                    onEdit = { editingCategory = category },
                    onDelete = { onDeleteCategory(category.id) },
                )
            }

            item {
                OutlinedButton(
                    onClick = { creatingCategory = true },
                    modifier = Modifier.fillMaxWidth(),
                ) { Text("Nouvelle catégorie") }
            }
        }
    }

    accepting?.let { suggestion ->
        AcceptSuggestionDialog(
            suggestion = suggestion,
            onDismiss = { accepting = null },
            onConfirm = { name, label ->
                onAccept(suggestion, name, label)
                accepting = null
            },
        )
    }

    editingRule?.let { rule ->
        RuleEditorDialog(
            rule = rule,
            categories = state.categories,
            onDismiss = { editingRule = null },
            onConfirm = {
                onSaveRule(it)
                editingRule = null
            },
        )
    }

    editingCategory?.let { category ->
        CategoryEditorDialog(
            initial = category,
            onDismiss = { editingCategory = null },
            onConfirm = { name, label, color ->
                onUpdateCategory(category.id, name, label, color)
                editingCategory = null
            },
        )
    }

    if (creatingCategory) {
        CategoryEditorDialog(
            initial = null,
            onDismiss = { creatingCategory = false },
            onConfirm = { name, label, _ ->
                onCreateCategory(name, label)
                creatingCategory = false
            },
        )
    }
}

@Composable
private fun SuggestionCard(suggestion: RuleSuggestion, onAccept: () -> Unit) {
    Card(
        Modifier.fillMaxWidth(),
        colors = CardDefaults.cardColors(
            containerColor = MaterialTheme.colorScheme.secondaryContainer,
        ),
    ) {
        Column(Modifier.padding(16.dp)) {
            Row(verticalAlignment = Alignment.CenterVertically) {
                Box(
                    Modifier
                        .size(12.dp)
                        .clip(CircleShape)
                        .background(Color(suggestion.suggestedColor.toInt())),
                )
                Spacer(Modifier.width(10.dp))
                Column(Modifier.weight(1f)) {
                    Text(
                        text = suggestion.label,
                        style = MaterialTheme.typography.titleSmall,
                        fontWeight = FontWeight.SemiBold,
                    )
                    Text(
                        text = "${suggestion.occurrences} séance(s)",
                        style = MaterialTheme.typography.bodySmall,
                        color = MaterialTheme.colorScheme.onSecondaryContainer.copy(alpha = 0.7f),
                    )
                }
                TextButton(onClick = onAccept) { Text("Créer") }
            }
            suggestion.samples.take(2).forEach { sample ->
                Text(
                    text = "· $sample",
                    style = MaterialTheme.typography.bodySmall,
                    maxLines = 1,
                    color = MaterialTheme.colorScheme.onSecondaryContainer.copy(alpha = 0.7f),
                )
            }
        }
    }
}

@Composable
private fun RuleCard(
    rule: Rule,
    category: Category?,
    onToggle: (Boolean) -> Unit,
    onEdit: () -> Unit,
    onDelete: () -> Unit,
) {
    Card(Modifier.fillMaxWidth()) {
        Column(Modifier.padding(16.dp)) {
            Row(verticalAlignment = Alignment.CenterVertically) {
                category?.let {
                    Box(
                        Modifier
                            .size(12.dp)
                            .clip(CircleShape)
                            .background(Color(it.color.toInt())),
                    )
                    Spacer(Modifier.width(10.dp))
                }
                Column(Modifier.weight(1f)) {
                    Text(
                        text = rule.name,
                        style = MaterialTheme.typography.titleSmall,
                        fontWeight = FontWeight.SemiBold,
                    )
                    Text(
                        text = describe(rule),
                        style = MaterialTheme.typography.bodySmall,
                        color = MaterialTheme.colorScheme.onSurface.copy(alpha = 0.65f),
                    )
                    Text(
                        text = "${rule.matchCount} séance(s) concernée(s)",
                        style = MaterialTheme.typography.bodySmall,
                        color = MaterialTheme.colorScheme.onSurface.copy(alpha = 0.5f),
                    )
                }
                Switch(checked = rule.enabled, onCheckedChange = onToggle)
            }
            Row(horizontalArrangement = Arrangement.End, modifier = Modifier.fillMaxWidth()) {
                TextButton(onClick = onEdit) { Text("Modifier") }
                TextButton(onClick = onDelete) { Text("Supprimer") }
            }
        }
    }
}

@Composable
private fun CategoryCard(
    category: Category,
    onEdit: () -> Unit,
    onDelete: () -> Unit,
) {
    Card(onClick = onEdit, modifier = Modifier.fillMaxWidth()) {
        Row(
            Modifier.padding(16.dp),
            verticalAlignment = Alignment.CenterVertically,
        ) {
            Box(
                Modifier
                    .size(14.dp)
                    .clip(CircleShape)
                    .background(Color(category.color.toInt())),
            )
            Spacer(Modifier.width(12.dp))
            Column(Modifier.weight(1f)) {
                Text(
                    text = category.name,
                    style = MaterialTheme.typography.titleSmall,
                    fontWeight = FontWeight.SemiBold,
                )
                Text(
                    text = "${category.occurrenceCount} séance(s)",
                    style = MaterialTheme.typography.bodySmall,
                    color = MaterialTheme.colorScheme.onSurface.copy(alpha = 0.6f),
                )
            }
            if (category.label.isNotBlank()) {
                CategoryChip(category.label, Color(category.color.toInt()))
                Spacer(Modifier.width(8.dp))
            }
            TextButton(onClick = onDelete) { Text("Supprimer") }
        }
    }
}

@Composable
private fun AcceptSuggestionDialog(
    suggestion: RuleSuggestion,
    onDismiss: () -> Unit,
    onConfirm: (String, String) -> Unit,
) {
    var name by remember { mutableStateOf(suggestion.pattern.uppercase()) }
    var label by remember { mutableStateOf(suggestion.pattern.take(3).uppercase()) }

    AlertDialog(
        onDismissRequest = onDismiss,
        title = { Text("Nouvelle catégorie") },
        text = {
            Column(verticalArrangement = Arrangement.spacedBy(12.dp)) {
                Text(
                    text = "${suggestion.occurrences} séance(s) contiennent « ${suggestion.pattern} ». " +
                        "Elles prendront cette couleur, et toutes celles qui arriveront ensuite.",
                    style = MaterialTheme.typography.bodySmall,
                )
                OutlinedTextField(
                    value = name,
                    onValueChange = { name = it },
                    label = { Text("Nom") },
                    singleLine = true,
                )
                OutlinedTextField(
                    value = label,
                    onValueChange = { label = it.take(4) },
                    label = { Text("Pastille (4 caractères)") },
                    singleLine = true,
                )
            }
        },
        confirmButton = {
            TextButton(
                onClick = { onConfirm(name.ifBlank { suggestion.pattern }, label) },
            ) { Text("Créer") }
        },
        dismissButton = { TextButton(onClick = onDismiss) { Text("Annuler") } },
    )
}

@Composable
private fun CategoryEditorDialog(
    initial: Category?,
    onDismiss: () -> Unit,
    onConfirm: (String, String, UInt) -> Unit,
) {
    var name by remember { mutableStateOf(initial?.name ?: "") }
    var label by remember { mutableStateOf(initial?.label ?: "") }
    var color by remember { mutableStateOf(initial?.color ?: PALETTE.first()) }

    AlertDialog(
        onDismissRequest = onDismiss,
        title = { Text(if (initial == null) "Nouvelle catégorie" else "Modifier la catégorie") },
        text = {
            Column(
                verticalArrangement = Arrangement.spacedBy(12.dp),
                modifier = Modifier.verticalScroll(rememberScrollState()),
            ) {
                OutlinedTextField(
                    value = name,
                    onValueChange = { name = it },
                    label = { Text("Nom") },
                    singleLine = true,
                )
                OutlinedTextField(
                    value = label,
                    onValueChange = { label = it.take(4) },
                    label = { Text("Pastille") },
                    singleLine = true,
                )
                if (initial != null) {
                    Text("Couleur", style = MaterialTheme.typography.labelMedium)
                    Row(horizontalArrangement = Arrangement.spacedBy(8.dp)) {
                        PALETTE.take(4).forEach { swatch ->
                            ColorDot(swatch, swatch == color) { color = swatch }
                        }
                    }
                    Row(horizontalArrangement = Arrangement.spacedBy(8.dp)) {
                        PALETTE.drop(4).forEach { swatch ->
                            ColorDot(swatch, swatch == color) { color = swatch }
                        }
                    }
                }
            }
        },
        confirmButton = {
            TextButton(onClick = { onConfirm(name, label, color) }) { Text("Enregistrer") }
        },
        dismissButton = { TextButton(onClick = onDismiss) { Text("Annuler") } },
    )
}

@Composable
private fun ColorDot(color: UInt, selected: Boolean, onClick: () -> Unit) {
    Box(
        Modifier
            .size(36.dp)
            .clip(CircleShape)
            .background(Color(color.toInt()))
            .border(
                width = if (selected) 3.dp else 0.dp,
                color = MaterialTheme.colorScheme.onSurface,
                shape = CircleShape,
            )
            .clickable(onClick = onClick),
    )
}

/** L'éditeur d'une règle, condition puis effets. */
@Composable
private fun RuleEditorDialog(
    rule: Rule,
    categories: List<Category>,
    onDismiss: () -> Unit,
    onConfirm: (Rule) -> Unit,
) {
    var draft by remember { mutableStateOf(rule) }

    AlertDialog(
        onDismissRequest = onDismiss,
        title = { Text(if (rule.id.isBlank()) "Nouvelle règle" else "Modifier la règle") },
        text = {
            Column(
                verticalArrangement = Arrangement.spacedBy(10.dp),
                modifier = Modifier.verticalScroll(rememberScrollState()),
            ) {
                OutlinedTextField(
                    value = draft.name,
                    onValueChange = { draft = draft.copy(name = it) },
                    label = { Text("Nom de la règle") },
                    singleLine = true,
                )
                OutlinedTextField(
                    value = draft.pattern,
                    onValueChange = { draft = draft.copy(pattern = it) },
                    label = { Text("Motif à reconnaître") },
                    singleLine = true,
                )

                Text("Dans", style = MaterialTheme.typography.labelMedium)
                Row(horizontalArrangement = Arrangement.spacedBy(6.dp)) {
                    fieldChip(draft, RuleField.TITLE, "Titre") { draft = it }
                    fieldChip(draft, RuleField.LOCATION, "Lieu") { draft = it }
                    fieldChip(draft, RuleField.ANY, "Partout") { draft = it }
                }

                Text("Comparaison", style = MaterialTheme.typography.labelMedium)
                Row(horizontalArrangement = Arrangement.spacedBy(6.dp)) {
                    matchChip(draft, RuleMatch.WORD, "Mot entier") { draft = it }
                    matchChip(draft, RuleMatch.CONTAINS, "Contient") { draft = it }
                    matchChip(draft, RuleMatch.STARTS_WITH, "Commence") { draft = it }
                }

                Text("Catégorie", style = MaterialTheme.typography.labelMedium)
                CategoryPicker(
                    categories = categories,
                    selected = draft.categoryId,
                    onSelect = { draft = draft.copy(categoryId = it) },
                )

                OutlinedTextField(
                    value = draft.renameTo.orEmpty(),
                    onValueChange = { draft = draft.copy(renameTo = it.ifBlank { null }) },
                    label = { Text("Renommer en (facultatif, {} = titre)") },
                    singleLine = true,
                )

                Row(verticalAlignment = Alignment.CenterVertically) {
                    Text("Masquer ces séances", Modifier.weight(1f))
                    Switch(
                        checked = draft.hide,
                        onCheckedChange = { draft = draft.copy(hide = it) },
                    )
                }
            }
        },
        confirmButton = {
            TextButton(
                onClick = { onConfirm(draft) },
                enabled = draft.pattern.isNotBlank(),
            ) { Text("Enregistrer") }
        },
        dismissButton = { TextButton(onClick = onDismiss) { Text("Annuler") } },
    )
}

@Composable
private fun fieldChip(draft: Rule, value: RuleField, label: String, onPick: (Rule) -> Unit) {
    FilterChip(
        selected = draft.field == value,
        onClick = { onPick(draft.copy(field = value)) },
        label = { Text(label) },
    )
}

@Composable
private fun matchChip(draft: Rule, value: RuleMatch, label: String, onPick: (Rule) -> Unit) {
    FilterChip(
        selected = draft.matchKind == value,
        onClick = { onPick(draft.copy(matchKind = value)) },
        label = { Text(label) },
    )
}

private fun emptyRule() = Rule(
    id = "",
    name = "",
    calendarId = null,
    field = RuleField.TITLE,
    matchKind = RuleMatch.WORD,
    pattern = "",
    caseSensitive = false,
    categoryId = null,
    renameTo = null,
    hide = false,
    priority = 0,
    enabled = true,
    matchCount = 0u,
)

/** « Titre · mot entier « TD » · masque » — la règle en une ligne. */
private fun describe(rule: Rule): String {
    val field = when (rule.field) {
        RuleField.TITLE -> "Titre"
        RuleField.LOCATION -> "Lieu"
        RuleField.DESCRIPTION -> "Notes"
        RuleField.ANY -> "Partout"
    }
    val match = when (rule.matchKind) {
        RuleMatch.CONTAINS -> "contient"
        RuleMatch.STARTS_WITH -> "commence par"
        RuleMatch.ENDS_WITH -> "finit par"
        RuleMatch.EQUALS -> "vaut"
        RuleMatch.WORD -> "contient le mot"
    }
    val effects = buildList {
        if (rule.categoryId != null) add("colorie")
        if (!rule.renameTo.isNullOrBlank()) add("renomme")
        if (rule.hide) add("masque")
    }
    val tail = if (effects.isEmpty()) "sans effet" else effects.joinToString(", ")
    return "$field $match « ${rule.pattern} » · $tail"
}
