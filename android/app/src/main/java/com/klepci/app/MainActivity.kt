package com.klepci.app

import android.os.Bundle
import androidx.activity.ComponentActivity
import androidx.activity.compose.setContent
import androidx.compose.foundation.background
import androidx.compose.foundation.clickable
import androidx.compose.foundation.layout.*
import androidx.compose.foundation.lazy.LazyColumn
import androidx.compose.foundation.lazy.items
import androidx.compose.material3.*
import androidx.compose.runtime.*
import androidx.compose.ui.Modifier
import androidx.compose.ui.graphics.Color
import androidx.compose.ui.platform.LocalContext
import androidx.compose.ui.unit.dp
import androidx.compose.ui.unit.sp
import org.json.JSONObject

private val Bg = Color(0xFF16181D)
private val Panel = Color(0xFF1E2128)
private val Accent = Color(0xFF7AA2F7)
private val Again = Color(0xFFF7768E)
private val Hard = Color(0xFFE0AF68)
private val Good = Color(0xFF9ECE6A)

class MainActivity : ComponentActivity() {
    override fun onCreate(savedInstanceState: Bundle?) {
        super.onCreate(savedInstanceState)
        setContent {
            MaterialTheme(colorScheme = darkColorScheme(
                background = Bg, surface = Panel, primary = Accent)) {
                KlepciApp()
            }
        }
    }
}

@Composable
fun KlepciApp() {
    val context = LocalContext.current
    var tab by remember { mutableStateOf(0) }
    var error by remember { mutableStateOf<String?>(null) }
    val core = remember {
        try {
            Klep.open(context.filesDir)
        } catch (e: Exception) {
            error = e.toString()
            null
        }
    }
    Column(Modifier.fillMaxSize().background(Bg)) {
        error?.let { Text(it, color = Again, modifier = Modifier.padding(12.dp)) }
        TabRow(selectedTabIndex = tab) {
            listOf("Decks", "Review", "Browse", "Stats").forEachIndexed { i, title ->
                Tab(selected = tab == i, onClick = { tab = i }, text = { Text(title) })
            }
        }
        when (tab) {
            0 -> Decks(core, { error = it }, { tab = 1 })
            1 -> Review(core, { error = it })
            2 -> Browse(core, { error = it })
            3 -> Stats(core, { error = it })
        }
    }
}

@Composable
fun Decks(core: uniffi.klep_mobile.MobileCore?, onError: (String) -> Unit, goReview: () -> Unit) {
    var decks by remember { mutableStateOf(listOf<uniffi.klep_mobile.DeckInfo>()) }
    LaunchedEffect(core) {
        core?.let {
            try { decks = it.decks() } catch (e: Exception) { onError(e.toString()) }
        }
    }
    if (decks.isEmpty()) {
        Box(Modifier.fillMaxSize(), contentAlignment = androidx.compose.ui.Alignment.Center) {
            Text("no decks yet", color = Color.Gray)
        }
        return
    }
    LazyColumn(Modifier.padding(12.dp)) {
        items(decks) { deck ->
            Card(Modifier.fillMaxWidth().padding(vertical = 5.dp)
                .clickable { goReview() }) {
                Text(deck.name, modifier = Modifier.padding(16.dp), fontSize = 16.sp)
            }
        }
    }
}

@Composable
fun Review(core: uniffi.klep_mobile.MobileCore?, onError: (String) -> Unit) {
    var card by remember { mutableStateOf<uniffi.klep_mobile.CardView?>(null) }
    var answer by remember { mutableStateOf<uniffi.klep_mobile.AnswerView?>(null) }
    var done by remember { mutableStateOf(false) }
    var shownAt by remember { mutableStateOf(0L) }

    fun next() {
        answer = null
        try {
            val next = core?.nextCard()
            if (next == null) { card = null; done = true } else {
                card = next; done = false; shownAt = System.currentTimeMillis()
            }
        } catch (e: Exception) { onError(e.toString()) }
    }
    LaunchedEffect(core) { if (card == null && !done) next() }

    Column(Modifier.fillMaxSize().padding(16.dp)) {
        when {
            done -> Box(Modifier.fillMaxSize(),
                contentAlignment = androidx.compose.ui.Alignment.Center) {
                Text("done for now 🎉", fontSize = 20.sp)
            }
            card == null -> Box(Modifier.fillMaxSize(),
                contentAlignment = androidx.compose.ui.Alignment.Center) {
                CircularProgressIndicator()
            }
            else -> {
                Card(Modifier.fillMaxWidth().weight(1f)) {
                    Text(htmlToText(card!!.question
                        + (answer?.let { "<hr>" + it.answer } ?: "")),
                        modifier = Modifier.padding(20.dp), fontSize = 19.sp)
                }
                Spacer(Modifier.height(12.dp))
                if (answer == null) {
                    Button(onClick = {
                        try {
                            answer = core?.showAnswer(card!!.id)
                        } catch (e: Exception) { onError(e.toString()) }
                    }, modifier = Modifier.fillMaxWidth().height(52.dp)) {
                        Text("Show answer")
                    }
                } else {
                    Row(Modifier.fillMaxWidth(), horizontalArrangement = Arrangement.spacedBy(8.dp)) {
                        listOf(Triple(1u, "Again", Again), Triple(2u, "Hard", Hard),
                            Triple(3u, "Good", Good), Triple(4u, "Easy", Accent)
                        ).forEach { (rating, label, color) ->
                            OutlinedButton(
                                onClick = {
                                    val secs = ((System.currentTimeMillis() - shownAt) / 1000)
                                        .coerceAtLeast(1).toUInt()
                                    try {
                                        core?.answer(card!!.id, rating, secs)
                                        next()
                                    } catch (e: Exception) { onError(e.toString()) }
                                },
                                modifier = Modifier.weight(1f).height(56.dp),
                                colors = ButtonDefaults.outlinedButtonColors(contentColor = color)
                            ) { Text(label) }
                        }
                    }
                }
            }
        }
    }
}

@Composable
fun Browse(core: uniffi.klep_mobile.MobileCore?, onError: (String) -> Unit) {
    var query by remember { mutableStateOf("") }
    var rows by remember { mutableStateOf(listOf<uniffi.klep_mobile.BrowserRow>()) }
    var selected by remember { mutableStateOf(setOf<Long>()) }
    var confirmDelete by remember { mutableStateOf(false) }

    fun search() {
        core?.let {
            try {
                rows = it.searchCards(query, 200u)
                selected = emptySet()
                confirmDelete = false
            } catch (e: Exception) { onError(e.toString()) }
        }
    }
    fun bulk(op: (List<Long>) -> Unit) {
        val ids = selected.toList()
        if (ids.isEmpty()) return
        try { op(ids); search() } catch (e: Exception) { onError(e.toString()) }
    }

    Column(Modifier.fillMaxSize().padding(12.dp)) {
        Row(horizontalArrangement = Arrangement.spacedBy(8.dp)) {
            OutlinedTextField(value = query, onValueChange = { query = it },
                label = { Text("search") }, modifier = Modifier.weight(1f))
            Button(onClick = ::search) { Text("Go") }
        }
        Spacer(Modifier.height(8.dp))
        Row(horizontalArrangement = Arrangement.spacedBy(8.dp)) {
            OutlinedButton(onClick = { selected = rows.map { it.id }.toSet() }) { Text("All") }
            OutlinedButton(onClick = { core?.let { bulk { ids -> it.suspendCards(ids, true) } } }) { Text("Susp") }
            OutlinedButton(onClick = { core?.let { bulk { ids -> it.suspendCards(ids, false) } } }) { Text("Uns") }
            OutlinedButton(onClick = { core?.let { bulk { ids -> it.buryCards(ids) } } }) { Text("Bury") }
            OutlinedButton(onClick = {
                if (confirmDelete) core?.let { bulk { ids -> it.deleteCards(ids) } }
                else confirmDelete = true
            }) { Text(if (confirmDelete) "Sure?" else "Del") }
        }
        Text("${selected.size} selected", color = Color.Gray,
            modifier = Modifier.padding(vertical = 6.dp))
        LazyColumn {
            items(rows) { row ->
                Row(Modifier.fillMaxWidth().padding(vertical = 4.dp)) {
                    Checkbox(
                        checked = row.id in selected,
                        onCheckedChange = { on ->
                            selected = if (on) selected + row.id else selected - row.id
                        }
                    )
                    Text("[${row.deck}] ${row.preview}", fontSize = 14.sp)
                }
            }
        }
    }
}

@Composable
fun Stats(core: uniffi.klep_mobile.MobileCore?, onError: (String) -> Unit) {
    var json by remember { mutableStateOf<JSONObject?>(null) }
    LaunchedEffect(core) {
        core?.let {
            try { json = JSONObject(it.timingJson("", 10u)) }
            catch (e: Exception) { onError(e.toString()) }
        }
    }
    val stats = json
    if (stats == null) {
        Box(Modifier.fillMaxSize(), contentAlignment = androidx.compose.ui.Alignment.Center) {
            Text("no stats yet", color = Color.Gray)
        }
        return
    }
    LazyColumn(Modifier.padding(16.dp)) {
        item {
            Text("${stats.getInt("reviews")} reviews · " +
                "mean ${formatSecs(stats.getDouble("mean_secs"))} · " +
                "total ${formatSecs(stats.getDouble("total_secs"))}")
            Spacer(Modifier.height(16.dp))
            Text("Slowest cards", style = MaterialTheme.typography.titleMedium)
        }
        val slowest = stats.getJSONArray("slowest")
        items((0 until slowest.length()).toList()) { i ->
            val card = slowest.getJSONObject(i)
            Text("#${card.getLong("card_id")} · " +
                "${card.getInt("reviews")} reps · " +
                "mean ${formatSecs(card.getDouble("mean_secs"))} · " +
                "worst ${formatSecs(card.getDouble("max_secs"))}",
                fontSize = 14.sp, modifier = Modifier.padding(vertical = 4.dp))
        }
    }
}
