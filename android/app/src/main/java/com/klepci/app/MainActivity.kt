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
            listOf("Decks", "Review", "Browse", "Add", "Stats").forEachIndexed { i, title ->
                Tab(selected = tab == i, onClick = { tab = i }, text = { Text(title) })
            }
        }
        when (tab) {
            0 -> Decks(core, { error = it }, { tab = 1 })
            1 -> Review(core, { error = it })
            2 -> Browse(core, { error = it })
            3 -> Add(core, { error = it })
            4 -> Stats(core, { error = it })
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
    var editState by remember { mutableStateOf<EditBuf?>(null) }

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
                OutlinedButton(
                    onClick = {
                        try {
                            editState = core?.editData(card!!.id)?.let {
                                EditBuf(it.noteId, it.fieldNames, it.fields.toMutableList(), it.tags)
                            }
                        } catch (e: Exception) { onError(e.toString()) }
                    },
                    modifier = Modifier.fillMaxWidth()
                ) { Text("Edit note") }
                Spacer(Modifier.height(8.dp))
                if (answer == null) {
                    Button(onClick = {
                        try {
                            answer = core?.showAnswer(card!!.id)
                        } catch (e: Exception) { onError(e.toString()) }
                    }, modifier = Modifier.fillMaxWidth().height(52.dp)) {
                        Text("Show answer")
                    }
                } else {
                    Row(Modifier.fillMaxWidth(), horizontalArrangement = Arrangement.spacedBy(8.dp)) {                        listOf(Triple(1u, "Again", Again), Triple(2u, "Hard", Hard),
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
    editState?.let { edit ->
        AlertDialog(
            onDismissRequest = { editState = null },
            title = { Text("Edit note") },
            text = {
                LazyColumn {
                    items(edit.fields.size) { i ->
                        OutlinedTextField(
                            value = edit.fields[i],
                            onValueChange = {
                                editState = edit.copy(
                                    fields = edit.fields.toMutableList().also { list -> list[i] = it })
                            },
                            label = { Text(edit.names.getOrElse(i) { "field $i" }) },
                            modifier = Modifier.fillMaxWidth().padding(vertical = 4.dp)
                        )
                    }
                    item {
                        OutlinedTextField(
                            value = edit.tags,
                            onValueChange = { editState = edit.copy(tags = it) },
                            label = { Text("tags") },
                            modifier = Modifier.fillMaxWidth().padding(vertical = 4.dp)
                        )
                    }
                }
            },
            confirmButton = {
                TextButton(onClick = {
                    try {
                        core?.saveNote(edit.noteId, edit.fields, edit.tags)
                        editState = null
                        answer = core?.showAnswer(card!!.id)
                    } catch (e: Exception) { onError(e.toString()) }
                }) { Text("Save") }
            },
            dismissButton = {
                TextButton(onClick = { editState = null }) { Text("Cancel") }
            }
        )
    }
}

data class EditBuf(
    val noteId: Long,
    val names: List<String>,
    val fields: List<String>,
    val tags: String,
)

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

@OptIn(ExperimentalMaterial3Api::class)
@Composable
fun Add(core: uniffi.klep_mobile.MobileCore?, onError: (String) -> Unit) {
    var notetypes by remember { mutableStateOf(listOf<String>()) }
    var decks by remember { mutableStateOf(listOf<uniffi.klep_mobile.DeckInfo>()) }
    var notetype by remember { mutableStateOf<String?>(null) }
    var deckId by remember { mutableStateOf<Long?>(null) }
    var fieldNames by remember { mutableStateOf(listOf<String>()) }
    var fields by remember { mutableStateOf(listOf<String>()) }
    var tags by remember { mutableStateOf("") }
    var status by remember { mutableStateOf("") }
    var ntOpen by remember { mutableStateOf(false) }
    var deckOpen by remember { mutableStateOf(false) }

    LaunchedEffect(core) {
        core?.let {
            try {
                notetypes = it.notetypeNames()
                decks = it.decks()
            } catch (e: Exception) { onError(e.toString()) }
        }
    }

    LazyColumn(Modifier.fillMaxSize().padding(16.dp)) {
        item {
            ExposedDropdownMenuBox(expanded = ntOpen, onExpandedChange = { ntOpen = it }) {
                OutlinedTextField(value = notetype ?: "", onValueChange = {},
                    readOnly = true, label = { Text("Notetype") },
                    trailingIcon = { ExposedDropdownMenuDefaults.TrailingIcon(ntOpen) },
                    modifier = Modifier.fillMaxWidth().menuAnchor())
                ExposedDropdownMenu(expanded = ntOpen, onDismissRequest = { ntOpen = false }) {
                    notetypes.forEach { name ->
                        DropdownMenuItem(text = { Text(name) }, onClick = {
                            notetype = name
                            ntOpen = false
                            try {
                                fieldNames = core?.fieldNames(name) ?: emptyList()
                                fields = List(fieldNames.size) { "" }
                            } catch (e: Exception) { onError(e.toString()) }
                        })
                    }
                }
            }
            Spacer(Modifier.height(8.dp))
            ExposedDropdownMenuBox(expanded = deckOpen, onExpandedChange = { deckOpen = it }) {
                OutlinedTextField(
                    value = decks.find { it.id == deckId }?.name ?: "",
                    onValueChange = {}, readOnly = true, label = { Text("Deck") },
                    trailingIcon = { ExposedDropdownMenuDefaults.TrailingIcon(deckOpen) },
                    modifier = Modifier.fillMaxWidth().menuAnchor())
                ExposedDropdownMenu(expanded = deckOpen, onDismissRequest = { deckOpen = false }) {
                    decks.forEach { deck ->
                        DropdownMenuItem(text = { Text(deck.name) }, onClick = {
                            deckId = deck.id
                            deckOpen = false
                        })
                    }
                }
            }
            Spacer(Modifier.height(8.dp))
        }
        items(fields.size) { i ->
            OutlinedTextField(value = fields[i],
                onValueChange = { fields = fields.toMutableList().also { list -> list[i] = it } },
                label = { Text(fieldNames.getOrElse(i) { "field $i" }) },
                modifier = Modifier.fillMaxWidth().padding(vertical = 4.dp))
        }
        if (fieldNames.isNotEmpty()) {
            item {
                OutlinedTextField(value = tags, onValueChange = { tags = it },
                    label = { Text("tags") }, modifier = Modifier.fillMaxWidth())
                Spacer(Modifier.height(8.dp))
                Button(onClick = {
                    try {
                        val id = core?.addNote(notetype!!, deckId!!, fields, tags)
                        status = "added note #$id"
                        fields = List(fields.size) { "" }
                        tags = ""
                    } catch (e: Exception) { onError(e.toString()) }
                }, modifier = Modifier.fillMaxWidth()) { Text("Add note") }
                if (status.isNotEmpty()) Text(status, color = Color.Gray)
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
