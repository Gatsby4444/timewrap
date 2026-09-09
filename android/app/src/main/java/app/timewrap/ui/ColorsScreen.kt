package app.timewrap.ui

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
import androidx.compose.foundation.layout.width
import androidx.compose.foundation.lazy.LazyColumn
import androidx.compose.foundation.lazy.items
import androidx.compose.material.icons.Icons
import androidx.compose.material.icons.automirrored.outlined.ArrowBack
import androidx.compose.material3.Card
import androidx.compose.material3.CardDefaults
import androidx.compose.material3.ExperimentalMaterial3Api
import androidx.compose.material3.Icon
import androidx.compose.material3.IconButton
import androidx.compose.material3.MaterialTheme
import androidx.compose.material3.OutlinedButton
import androidx.compose.material3.Scaffold
import androidx.compose.material3.Text
import androidx.compose.material3.TopAppBar
import androidx.compose.runtime.Composable
import androidx.compose.runtime.LaunchedEffect
import androidx.compose.runtime.getValue
import androidx.compose.runtime.mutableStateOf
import androidx.compose.runtime.remember
import androidx.compose.runtime.setValue
import androidx.compose.ui.Alignment
import androidx.compose.ui.Modifier
import androidx.compose.ui.text.font.FontWeight
import androidx.compose.ui.unit.dp
import app.timewrap.UiState
import app.timewrap.core.PropertyKey
import app.timewrap.core.PropertyValue

/**
 * Le choix du champ à colorier.
 *
 * L'ENT range le type de cours et la matière dans la description, en clair.
 * Plutôt que de faire deviner une règle à l'utilisateur, on lui montre ce que
 * son fichier contient déjà : « Type — 3 valeurs », « Matière — 7 valeurs », et
 * il choisit lequel doit porter les couleurs.
 */
@OptIn(ExperimentalMaterial3Api::class)
@Composable
fun ColorsScreen(
    state: UiState,
    onLoad: () -> Unit,
    onOpenKey: (PropertyKey) -> Unit,
    onRules: () -> Unit,
    onBack: () -> Unit,
) {
    LaunchedEffect(Unit) { onLoad() }

    Scaffold(
        topBar = {
            TopAppBar(
                title = { Text("Couleurs") },
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
            item {
                Text(
                    text = "Colorier par…",
                    style = MaterialTheme.typography.titleMedium,
                    fontWeight = FontWeight.SemiBold,
                )
                Text(
                    text = "Ces champs ont été lus dans la description de vos cours. " +
                        "Choisissez celui qui doit décider des couleurs.",
                    style = MaterialTheme.typography.bodySmall,
                    color = MaterialTheme.colorScheme.onBackground.copy(alpha = 0.6f),
                )
            }

            if (state.propertyKeys.isEmpty()) {
                item {
                    Spacer(Modifier.height(16.dp))
                    Text(
                        text = "Aucun champ repéré.",
                        style = MaterialTheme.typography.titleSmall,
                        fontWeight = FontWeight.SemiBold,
                    )
                    Text(
                        text = "Votre export ne détaille pas ses séances sous la forme " +
                            "« Type : TD ». Les règles écrites à la main, qui travaillent " +
                            "sur l'intitulé, restent disponibles.",
                        style = MaterialTheme.typography.bodySmall,
                        color = MaterialTheme.colorScheme.onBackground.copy(alpha = 0.6f),
                    )
                }
            }

            items(state.propertyKeys, key = { it.key }) { key ->
                PropertyKeyCard(key) { onOpenKey(key) }
            }

            item {
                Spacer(Modifier.height(8.dp))
                OutlinedButton(onClick = onRules, modifier = Modifier.fillMaxWidth()) {
                    Text("Règles avancées")
                }
                Text(
                    text = "Pour ce que les champs ne couvrent pas : renommer, masquer, " +
                        "ou reconnaître un mot dans l'intitulé.",
                    style = MaterialTheme.typography.bodySmall,
                    color = MaterialTheme.colorScheme.onBackground.copy(alpha = 0.6f),
                )
            }
        }
    }
}

@Composable
private fun PropertyKeyCard(key: PropertyKey, onClick: () -> Unit) {
    Card(onClick = onClick, modifier = Modifier.fillMaxWidth()) {
        Column(Modifier.padding(16.dp)) {
            Text(
                text = key.label,
                style = MaterialTheme.typography.titleMedium,
                fontWeight = FontWeight.SemiBold,
            )
            Text(
                text = buildString {
                    append("${key.distinctValues} valeur(s) · ${key.occurrences} séance(s)")
                    if (key.coloredValues > 0u) {
                        append(" · ${key.coloredValues} coloriée(s)")
                    }
                },
                style = MaterialTheme.typography.bodySmall,
                color = MaterialTheme.colorScheme.onSurface.copy(alpha = 0.6f),
            )
        }
    }
}

/**
 * Les valeurs d'un champ, une couleur par ligne.
 *
 * C'est l'écran qui fait le travail : une matière, une pastille. « Tout
 * colorier » fait le premier jet — huit couleurs distribuées d'un coup — et il
 * ne reste qu'à corriger celles qu'on n'aime pas.
 */
@OptIn(ExperimentalMaterial3Api::class)
@Composable
fun PropertyValuesScreen(
    propertyKey: PropertyKey,
    values: List<PropertyValue>,
    onLoad: () -> Unit,
    onPick: (String, UInt) -> Unit,
    onClear: (String) -> Unit,
    onAutoColor: () -> Unit,
    onBack: () -> Unit,
) {
    var editing by remember { mutableStateOf<PropertyValue?>(null) }

    LaunchedEffect(propertyKey.key) { onLoad() }

    Scaffold(
        topBar = {
            TopAppBar(
                title = { Text(propertyKey.label) },
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
            verticalArrangement = Arrangement.spacedBy(8.dp),
        ) {
            item {
                OutlinedButton(onClick = onAutoColor, modifier = Modifier.fillMaxWidth()) {
                    Text("Tout colorier")
                }
                Text(
                    text = "Une couleur différente par valeur, d'un coup. " +
                        "Vous pourrez ensuite changer celles qui ne vous plaisent pas.",
                    style = MaterialTheme.typography.bodySmall,
                    color = MaterialTheme.colorScheme.onBackground.copy(alpha = 0.6f),
                )
                Spacer(Modifier.height(6.dp))
            }

            items(values, key = { it.value }) { value ->
                PropertyValueRow(value) { editing = value }
            }
        }
    }

    editing?.let { value ->
        ColorPickerDialog(
            title = value.value,
            current = value.color.takeIf { value.colored },
            onDismiss = { editing = null },
            onPick = {
                onPick(value.value, it)
                editing = null
            },
            onClear = if (value.colored) {
                {
                    onClear(value.value)
                    editing = null
                }
            } else {
                null
            },
        )
    }
}

@Composable
private fun PropertyValueRow(value: PropertyValue, onClick: () -> Unit) {
    Card(
        onClick = onClick,
        modifier = Modifier.fillMaxWidth(),
        colors = CardDefaults.cardColors(
            containerColor = MaterialTheme.colorScheme.surface,
        ),
    ) {
        Row(
            Modifier.padding(horizontal = 16.dp, vertical = 12.dp),
            verticalAlignment = Alignment.CenterVertically,
        ) {
            ColorDot(
                color = value.color,
                selected = false,
                size = 28,
                onClick = onClick,
            )
            Spacer(Modifier.width(14.dp))
            Column(Modifier.weight(1f)) {
                Text(
                    text = value.value,
                    style = MaterialTheme.typography.bodyLarge,
                    fontWeight = FontWeight.Medium,
                )
                Text(
                    text = buildString {
                        append("${value.occurrences} séance(s)")
                        if (!value.colored) append(" · sans couleur propre")
                    },
                    style = MaterialTheme.typography.bodySmall,
                    color = MaterialTheme.colorScheme.onSurface.copy(alpha = 0.6f),
                )
            }
            Box {}
        }
    }
}
