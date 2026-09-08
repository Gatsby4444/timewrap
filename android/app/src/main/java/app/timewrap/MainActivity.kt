package app.timewrap

import android.os.Bundle
import androidx.activity.ComponentActivity
import androidx.activity.compose.setContent
import androidx.activity.enableEdgeToEdge
import androidx.compose.foundation.layout.Arrangement
import androidx.compose.foundation.layout.Column
import androidx.compose.foundation.layout.PaddingValues
import androidx.compose.foundation.layout.Row
import androidx.compose.foundation.layout.Spacer
import androidx.compose.foundation.layout.fillMaxSize
import androidx.compose.foundation.layout.fillMaxWidth
import androidx.compose.foundation.layout.height
import androidx.compose.foundation.layout.padding
import androidx.compose.foundation.lazy.LazyColumn
import androidx.compose.foundation.lazy.LazyListScope
import androidx.compose.material3.Card
import androidx.compose.material3.CardDefaults
import androidx.compose.material3.MaterialTheme
import androidx.compose.material3.Scaffold
import androidx.compose.material3.Surface
import androidx.compose.material3.Text
import androidx.compose.runtime.Composable
import androidx.compose.runtime.remember
import androidx.compose.ui.Alignment
import androidx.compose.ui.Modifier
import androidx.compose.ui.text.font.FontFamily
import androidx.compose.ui.text.font.FontWeight
import androidx.compose.ui.unit.dp
import app.timewrap.core.SelfTest
import app.timewrap.core.selfTest
import app.timewrap.ui.theme.TimewrapTheme

class MainActivity : ComponentActivity() {
    override fun onCreate(savedInstanceState: Bundle?) {
        super.onCreate(savedInstanceState)
        enableEdgeToEdge()
        setContent {
            TimewrapTheme {
                Surface(
                    modifier = Modifier.fillMaxSize(),
                    color = MaterialTheme.colorScheme.background,
                ) {
                    DiagnosticScreen()
                }
            }
        }
    }
}

/**
 * Ecran de la phase 0 : il ne fait qu'une chose, mais elle compte - executer le
 * coeur Rust sur l'appareil et montrer que chaque brique native repond.
 */
@Composable
private fun DiagnosticScreen() {
    val report = remember { runCatching { selfTest() } }

    Scaffold { innerPadding ->
        LazyColumn(
            modifier = Modifier.fillMaxSize(),
            contentPadding = PaddingValues(
                start = 20.dp,
                end = 20.dp,
                top = innerPadding.calculateTopPadding() + 24.dp,
                bottom = innerPadding.calculateBottomPadding() + 24.dp,
            ),
            verticalArrangement = Arrangement.spacedBy(12.dp),
        ) {
            item {
                Column {
                    Text(
                        text = "Timewrap",
                        style = MaterialTheme.typography.headlineMedium,
                        fontWeight = FontWeight.Bold,
                    )
                    Text(
                        text = "Diagnostic de la pile native",
                        style = MaterialTheme.typography.bodyMedium,
                        color = MaterialTheme.colorScheme.onBackground.copy(alpha = 0.7f),
                    )
                    Spacer(Modifier.height(12.dp))
                }
            }

            report.fold(
                onSuccess = { selfTestBody(it) },
                onFailure = { error ->
                    item {
                        StatusCard(
                            ok = false,
                            title = "Le coeur Rust n'a pas pu etre charge",
                            detail = error.message ?: error.toString(),
                        )
                    }
                },
            )
        }
    }
}

private fun LazyListScope.selfTestBody(report: SelfTest) {
    val ok = report.failures.isEmpty()

    item {
        StatusCard(
            ok = ok,
            title = if (ok) {
                "Toutes les briques repondent"
            } else {
                report.failures.size.toString() + " brique(s) en echec"
            },
            detail = if (ok) {
                "SQLite, fuseaux horaires, lecture iCalendar et recurrences fonctionnent sur cet appareil."
            } else {
                report.failures.joinToString("\n")
            },
        )
    }

    item { SectionTitle("Coeur") }
    item { InfoRow("Version", report.coreVersion) }
    item { InfoRow("SQLite", report.sqliteVersion) }

    item { SectionTitle("Horloge et fuseaux") }
    item { InfoRow("Maintenant (UTC)", report.nowUtc) }
    item { InfoRow("Maintenant (Paris)", report.nowParis) }

    item { SectionTitle("Lecture iCalendar") }
    item { InfoRow("Evenements lus", report.icsEventCount.toString()) }
    item { InfoRow("Intitule", report.icsFirstSummary) }
    item { InfoRow("Lieu", report.icsFirstLocation) }

    item { SectionTitle("Recurrences") }
    item { InfoRow("Occurrences", report.rruleOccurrenceCount.toString()) }
    item { InfoRow("Premiere", report.rruleFirst) }
    item { InfoRow("Derniere", report.rruleLast) }
}

@Composable
private fun StatusCard(ok: Boolean, title: String, detail: String) {
    Card(
        modifier = Modifier.fillMaxWidth(),
        colors = CardDefaults.cardColors(
            containerColor = if (ok) {
                MaterialTheme.colorScheme.primaryContainer
            } else {
                MaterialTheme.colorScheme.errorContainer
            },
        ),
    ) {
        Column(Modifier.padding(16.dp)) {
            Text(
                text = (if (ok) "OK  " else "KO  ") + title,
                style = MaterialTheme.typography.titleMedium,
                fontWeight = FontWeight.SemiBold,
            )
            Spacer(Modifier.height(6.dp))
            Text(text = detail, style = MaterialTheme.typography.bodyMedium)
        }
    }
}

@Composable
private fun SectionTitle(text: String) {
    Text(
        text = text.uppercase(),
        style = MaterialTheme.typography.labelMedium,
        color = MaterialTheme.colorScheme.onBackground.copy(alpha = 0.55f),
        modifier = Modifier.padding(top = 12.dp, start = 4.dp),
    )
}

@Composable
private fun InfoRow(label: String, value: String) {
    Card(
        modifier = Modifier.fillMaxWidth(),
        colors = CardDefaults.cardColors(containerColor = MaterialTheme.colorScheme.surface),
    ) {
        Row(
            modifier = Modifier
                .fillMaxWidth()
                .padding(horizontal = 16.dp, vertical = 12.dp),
            horizontalArrangement = Arrangement.SpaceBetween,
            verticalAlignment = Alignment.CenterVertically,
        ) {
            Text(
                text = label,
                style = MaterialTheme.typography.bodyMedium,
                color = MaterialTheme.colorScheme.onSurface.copy(alpha = 0.7f),
            )
            Text(
                text = value.ifBlank { "-" },
                style = MaterialTheme.typography.bodyMedium,
                fontFamily = FontFamily.Monospace,
                fontWeight = FontWeight.Medium,
            )
        }
    }
}
