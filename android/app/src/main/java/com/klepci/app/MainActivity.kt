package com.klepci.app

import android.os.Bundle
import androidx.activity.ComponentActivity
import androidx.activity.compose.rememberLauncherForActivityResult
import androidx.activity.compose.setContent
import androidx.activity.result.contract.ActivityResultContracts
import androidx.compose.foundation.Canvas
import androidx.compose.foundation.background
import androidx.compose.foundation.clickable
import androidx.compose.foundation.horizontalScroll
import androidx.compose.foundation.rememberScrollState
import androidx.compose.foundation.layout.*
import androidx.compose.foundation.lazy.LazyColumn
import androidx.compose.foundation.lazy.items
import androidx.compose.material3.*
import androidx.compose.runtime.*
import androidx.compose.ui.Modifier
import androidx.compose.ui.geometry.Offset
import androidx.compose.ui.geometry.Size
import androidx.compose.ui.graphics.Color
import androidx.compose.ui.platform.LocalContext
import androidx.compose.ui.unit.dp
import androidx.compose.ui.unit.sp
import java.io.File
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
    var decks by remember { mutableStateOf(listOf<uniffi.klep_mobile.DeckCounts>()) }
    var editName by remember { mutableStateOf<String?>(null) }
    var renameId by remember { mutableStateOf<Long?>(null) }
    var confirmDelete by remember { mutableStateOf<Long?>(null) }
    var status by remember { mutableStateOf("") }
    var options by remember { mutableStateOf<uniffi.klep_mobile.DeckOptions?>(null) }

    fun refresh() {
        core?.let {
            try { decks = it.deckOverview() } catch (e: Exception) { onError(e.toString()) }
        }
    }
    LaunchedEffect(core) { refresh() }

    val context = LocalContext.current
    val importLauncher = rememberLauncherForActivityResult(
        ActivityResultContracts.OpenDocument()
    ) { uri ->
        if (uri != null) {
            try {
                val tmp = File(context.cacheDir, "klepci-import.apkg")
                context.contentResolver.openInputStream(uri)?.use { input ->
                    tmp.outputStream().use { output -> input.copyTo(output) }
                }
                val count = core?.importApkg(tmp.absolutePath)
                status = "imported $count new notes"
                refresh()
            } catch (e: Exception) { onError(e.toString()) }
        }
    }
    val exportLauncher = rememberLauncherForActivityResult(
        ActivityResultContracts.CreateDocument("application/octet-stream")
    ) { uri ->
        if (uri != null) {
            try {
                val tmp = File(context.cacheDir, "klepci-export.apkg")
                val count = core?.exportApkg(tmp.absolutePath, "")
                context.contentResolver.openOutputStream(uri)?.use { output ->
                    tmp.inputStream().use { input -> input.copyTo(output) }
                }
                status = "exported $count notes"
            } catch (e: Exception) { onError(e.toString()) }
        }
    }

    Column(Modifier.fillMaxSize().padding(12.dp)) {
        OutlinedButton(
            onClick = { editName = ""; renameId = null },
            modifier = Modifier.fillMaxWidth()
        ) { Text("New deck") }
        Row(Modifier.fillMaxWidth(), horizontalArrangement = Arrangement.spacedBy(8.dp)) {
            OutlinedButton(
                onClick = { importLauncher.launch(arrayOf("*/*")) },
                modifier = Modifier.weight(1f)
            ) { Text("Import .apkg") }
            OutlinedButton(
                onClick = { exportLauncher.launch("klepci-export.apkg") },
                modifier = Modifier.weight(1f)
            ) { Text("Export .apkg") }
        }
        if (status.isNotEmpty()) {
            Text(status, color = Color.Gray, fontSize = 12.sp,
                modifier = Modifier.padding(vertical = 4.dp))
        }
        if (decks.isEmpty()) {
            Box(Modifier.fillMaxSize(), contentAlignment = androidx.compose.ui.Alignment.Center) {
                Text("no decks yet", color = Color.Gray)
            }
        } else {
            LazyColumn {
                items(decks) { deck ->
                    Card(Modifier.fillMaxWidth().padding(vertical = 4.dp)) {
                        Column(Modifier.padding(12.dp)) {
                            Text(
                                (if (deck.filtered) "▣ " else "") + deck.name,
                                fontSize = 16.sp,
                                modifier = Modifier.clickable {
                                    try { core?.selectDeck(deck.id) }
                                    catch (e: Exception) { onError(e.toString()) }
                                    goReview()
                                })
                            Text(
                                "${deck.new} new · ${deck.learn} learn · " +
                                    "${deck.review} review · ${deck.total} cards",
                                color = if (deck.new + deck.learn + deck.review == 0)
                                    Color.Gray else Accent,
                                fontSize = 12.sp,
                                modifier = Modifier.padding(top = 2.dp))
                            Row(horizontalArrangement = Arrangement.spacedBy(4.dp)) {
                                TextButton(onClick = {
                                    editName = deck.name; renameId = deck.id
                                }) { Text("Rename") }
                                TextButton(onClick = {
                                    try { options = core?.deckOptions(deck.id) }
                                    catch (e: Exception) { onError(e.toString()) }
                                }) { Text("Options") }
                                TextButton(onClick = {
                                    if (confirmDelete == deck.id) {
                                        try {
                                            core?.deleteDeck(deck.id)
                                            status = "deleted ${deck.name}"
                                            confirmDelete = null
                                            refresh()
                                        } catch (e: Exception) { onError(e.toString()) }
                                    } else {
                                        confirmDelete = deck.id
                                        status = "tap Delete again to remove ${deck.name}"
                                    }
                                }) {
                                    Text(if (confirmDelete == deck.id) "Confirm" else "Delete",
                                        color = Again)
                                }
                            }
                        }
                    }
                }
            }
        }
    }

    options?.let { opts ->
        AlertDialog(
            onDismissRequest = { options = null },
            title = { Text("Options \u00b7 ${opts.presetName}") },
            text = {
                LazyColumn(Modifier.heightIn(max = 420.dp)) {
                    item {
                        Text("Changes apply to every deck using this preset.",
                            fontSize = 12.sp, color = Color.Gray)
                    }
                    item {
                        NumberField("New cards/day", opts.newPerDay.toString()) { v ->
                            options = opts.copy(newPerDay = v.toUIntOrNull() ?: opts.newPerDay)
                        }
                    }
                    item {
                        NumberField("Maximum reviews/day", opts.reviewsPerDay.toString()) { v ->
                            options = opts.copy(reviewsPerDay = v.toUIntOrNull() ?: opts.reviewsPerDay)
                        }
                    }
                    item {
                        NumberField("Learning steps (min)", opts.learnSteps) { v ->
                            options = opts.copy(learnSteps = v)
                        }
                    }
                    item {
                        NumberField("Relearning steps (min)", opts.relearnSteps) { v ->
                            options = opts.copy(relearnSteps = v)
                        }
                    }
                    item {
                        NumberField("Graduating interval (good)", opts.graduatingIntervalGood.toString()) { v ->
                            options = opts.copy(graduatingIntervalGood = v.toUIntOrNull() ?: opts.graduatingIntervalGood)
                        }
                    }
                    item {
                        NumberField("Easy interval", opts.graduatingIntervalEasy.toString()) { v ->
                            options = opts.copy(graduatingIntervalEasy = v.toUIntOrNull() ?: opts.graduatingIntervalEasy)
                        }
                    }
                    item {
                        NumberField("Easy bonus", opts.easyMultiplier.toString()) { v ->
                            options = opts.copy(easyMultiplier = v.toFloatOrNull() ?: opts.easyMultiplier)
                        }
                    }
                    item {
                        NumberField("Hard interval", opts.hardMultiplier.toString()) { v ->
                            options = opts.copy(hardMultiplier = v.toFloatOrNull() ?: opts.hardMultiplier)
                        }
                    }
                    item {
                        NumberField("Interval modifier", opts.intervalMultiplier.toString()) { v ->
                            options = opts.copy(intervalMultiplier = v.toFloatOrNull() ?: opts.intervalMultiplier)
                        }
                    }
                    item {
                        NumberField("Maximum interval (days)", opts.maximumReviewInterval.toString()) { v ->
                            options = opts.copy(maximumReviewInterval = v.toUIntOrNull() ?: opts.maximumReviewInterval)
                        }
                    }
                    item {
                        NumberField("Leech threshold", opts.leechThreshold.toString()) { v ->
                            options = opts.copy(leechThreshold = v.toUIntOrNull() ?: opts.leechThreshold)
                        }
                    }
                    item {
                        NumberField("Desired retention", opts.desiredRetention.toString()) { v ->
                            options = opts.copy(desiredRetention = v.toFloatOrNull() ?: opts.desiredRetention)
                        }
                    }
                    item {
                        Row(verticalAlignment = androidx.compose.ui.Alignment.CenterVertically) {
                            Switch(checked = opts.fsrs, onCheckedChange = { options = opts.copy(fsrs = it) })
                            Text("FSRS scheduler", modifier = Modifier.padding(start = 8.dp))
                        }
                    }
                    item {
                        Row(verticalAlignment = androidx.compose.ui.Alignment.CenterVertically) {
                            Switch(checked = opts.showTimer, onCheckedChange = { options = opts.copy(showTimer = it) })
                            Text("Show answer timer", modifier = Modifier.padding(start = 8.dp))
                        }
                    }
                }
            },
            confirmButton = {
                TextButton(onClick = {
                    try {
                        options?.let { core?.saveDeckOptions(it) }
                        options = null
                        status = "options saved"
                        refresh()
                    } catch (e: Exception) { onError(e.toString()) }
                }) { Text("Save") }
            },
            dismissButton = {
                TextButton(onClick = { options = null }) { Text("Cancel") }
            })
    }

    editName?.let { name ->
        AlertDialog(
            onDismissRequest = { editName = null },
            title = { Text(if (renameId == null) "New deck" else "Rename deck") },
            text = {
                OutlinedTextField(
                    value = name,
                    onValueChange = { editName = it },
                    label = { Text("Deck name (A::B for subdecks)") },
                    singleLine = true)
            },
            confirmButton = {
                TextButton(onClick = {
                    try {
                        if (renameId == null) {
                            status = "created deck #${core?.addDeck(name)}"
                        } else {
                            core?.renameDeck(renameId!!, name)
                            status = "renamed deck"
                        }
                        editName = null; renameId = null
                        refresh()
                    } catch (e: Exception) { onError(e.toString()) }
                }) { Text("Save") }
            },
            dismissButton = {
                TextButton(onClick = { editName = null; renameId = null }) { Text("Cancel") }
            })
    }
}

@Composable
fun NumberField(label: String, value: String, onChange: (String) -> Unit) {
    OutlinedTextField(
        value = value,
        onValueChange = onChange,
        label = { Text(label) },
        singleLine = true,
        modifier = Modifier.fillMaxWidth().padding(vertical = 4.dp))
}

@Composable
fun Review(core: uniffi.klep_mobile.MobileCore?, onError: (String) -> Unit) {
    var card by remember { mutableStateOf<uniffi.klep_mobile.CardView?>(null) }
    var answer by remember { mutableStateOf<uniffi.klep_mobile.AnswerView?>(null) }
    var done by remember { mutableStateOf(false) }
    var shownAt by remember { mutableStateOf(0L) }
    var editState by remember { mutableStateOf<EditBuf?>(null) }
    var marked by remember { mutableStateOf(false) }
    var flag by remember { mutableStateOf(0) }

    fun next() {
        answer = null
        try {
            val next = core?.nextCard()
            if (next == null) { card = null; done = true } else {
                card = next; done = false; shownAt = System.currentTimeMillis()
                marked = core?.isMarked(next.id) ?: false
                flag = 0
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
                Row(Modifier.fillMaxWidth(), horizontalArrangement = Arrangement.spacedBy(6.dp)) {
                    TextButton(onClick = {
                        try { core?.undo(); next() } catch (e: Exception) { onError(e.toString()) }
                    }) { Text("Undo") }
                    TextButton(onClick = {
                        try { core?.suspend(listOf(card!!.id), true); next() }
                        catch (e: Exception) { onError(e.toString()) }
                    }) { Text("Suspend") }
                    TextButton(onClick = {
                        try { core?.bury(listOf(card!!.id)); next() }
                        catch (e: Exception) { onError(e.toString()) }
                    }) { Text("Bury") }
                    TextButton(onClick = {
                        try { marked = core?.toggleMark(card!!.id) ?: marked }
                        catch (e: Exception) { onError(e.toString()) }
                    }) { Text(if (marked) "Unmark" else "Mark") }
                }
                Row(
                    Modifier.fillMaxWidth().horizontalScroll(rememberScrollState()),
                    horizontalArrangement = Arrangement.spacedBy(2.dp)
                ) {
                    Text("flag:", modifier = Modifier.padding(top = 10.dp), fontSize = 12.sp)
                    (0..7).forEach { value ->
                        TextButton(onClick = {
                            try {
                                flag = value
                                core?.flagCard(card!!.id, value.toUInt())
                            } catch (e: Exception) { onError(e.toString()) }
                        }, colors = if (flag == value)
                            ButtonDefaults.textButtonColors(contentColor = Accent)
                        else ButtonDefaults.textButtonColors()) { Text("$value") }
                    }
                }
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
    var showReplace by remember { mutableStateOf(false) }
    var regex by remember { mutableStateOf("") }
    var replacement by remember { mutableStateOf("") }
    var replaceField by remember { mutableStateOf("") }
    var replaceStatus by remember { mutableStateOf("") }

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
            OutlinedButton(onClick = { showReplace = true }) { Text("Replace") }
        }
        if (replaceStatus.isNotEmpty()) {
            Text(replaceStatus, color = Color.Gray, fontSize = 12.sp)
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

    if (showReplace) {
        AlertDialog(
            onDismissRequest = { showReplace = false },
            title = { Text("Find & replace in ${rows.size} results") },
            text = {
                Column {
                    OutlinedTextField(value = regex, onValueChange = { regex = it },
                        label = { Text("find (regex)") }, singleLine = true,
                        modifier = Modifier.fillMaxWidth())
                    OutlinedTextField(value = replacement, onValueChange = { replacement = it },
                        label = { Text("replace with") }, singleLine = true,
                        modifier = Modifier.fillMaxWidth())
                    OutlinedTextField(value = replaceField, onValueChange = { replaceField = it },
                        label = { Text("field name (optional)") }, singleLine = true,
                        modifier = Modifier.fillMaxWidth())
                }
            },
            confirmButton = {
                TextButton(onClick = {
                    try {
                        val count = core?.findAndReplace(
                            query, regex, replacement,
                            replaceField.takeIf { it.isNotBlank() })
                        replaceStatus = "replaced in $count notes"
                        showReplace = false
                        search()
                    } catch (e: Exception) { onError(e.toString()) }
                }) { Text("Replace all") }
            },
            dismissButton = {
                TextButton(onClick = { showReplace = false }) { Text("Cancel") }
            })
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
@Composable
fun Spark(values: List<Int>, color: Color) {
    Canvas(Modifier.fillMaxWidth().height(64.dp)) {
        val max = (values.maxOrNull() ?: 1).coerceAtLeast(1)
        val step = size.width / values.size.coerceAtLeast(1)
        values.forEachIndexed { index, value ->
            val h = size.height * value / max
            if (h > 0f) {
                drawRect(color,
                    topLeft = Offset(index * step + 1f, size.height - h),
                    size = Size((step - 2f).coerceAtLeast(1f), h))
            }
        }
    }
}

@Composable
fun HBar(label: String, value: Int, max: Int, color: Color) {
    Row(verticalAlignment = androidx.compose.ui.Alignment.CenterVertically,
        modifier = Modifier.padding(vertical = 2.dp)) {
        Text(label, fontSize = 11.sp, color = Color.Gray,
            modifier = Modifier.width(76.dp))
        Box(Modifier.weight(1f).height(12.dp).background(Panel)) {
            if (max > 0 && value > 0) {
                Box(Modifier.fillMaxWidth(value.toFloat() / max)
                    .fillMaxHeight().background(color))
            }
        }
        Text(" $value", fontSize = 11.sp)
    }
}

@Composable
fun Stats(core: uniffi.klep_mobile.MobileCore?, onError: (String) -> Unit) {
    var json by remember { mutableStateOf<JSONObject?>(null) }
    var graphs by remember { mutableStateOf<uniffi.klep_mobile.GraphsData?>(null) }
    LaunchedEffect(core) {
        core?.let {
            try {
                json = JSONObject(it.timingJson("", 10u))
                graphs = it.graphs("", 365u)
            } catch (e: Exception) { onError(e.toString()) }
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
            Spacer(Modifier.height(12.dp))
        }
        graphs?.let { g ->
            item {
                val today = g.today
                val minutes = today.answerMillis / 60000.0
                val accuracy = if (today.answerCount == 0u) 0.0
                    else today.correctCount.toDouble() / today.answerCount * 100.0
                Text("Today", style = MaterialTheme.typography.titleMedium)
                Text(String.format(java.util.Locale.US,
                    "%.0f answers · %.1f min · %.0f%% correct",
                    today.answerCount.toDouble(), minutes, accuracy),
                    fontSize = 13.sp, color = Color.Gray)
                Spacer(Modifier.height(8.dp))
                Text("Cards", style = MaterialTheme.typography.titleMedium)
                val counts = g.cardCounts
                Text("${counts.new} new · ${counts.learn} learn · " +
                    "${counts.young} young · ${counts.mature} mature · " +
                    "${counts.suspended} suspended · ${counts.buried} buried",
                    fontSize = 12.sp, color = Color.Gray)
                Spacer(Modifier.height(10.dp))
                Text("Reviews/day (30d)", style = MaterialTheme.typography.titleSmall)
                val reviews = g.reviews.associateBy { it.day }
                Spark((0 until 30).map { offset ->
                    val day = reviews[(offset - 29).toLong()]
                    if (day == null) 0 else
                        (day.learn + day.relearn + day.young + day.mature + day.filtered).toInt()
                }, Accent)
                Text("Added/day (30d)", style = MaterialTheme.typography.titleSmall)
                val added = g.added.associateBy { it.day }
                Spark((0 until 30).map { offset ->
                    added[(offset - 29).toLong()]?.count?.toInt() ?: 0
                }, Hard)
                Text("Future due (30d)", style = MaterialTheme.typography.titleSmall)
                val due = g.futureDue.associateBy { it.day }
                Spark((0 until 30).map { due[it.toLong()]?.count?.toInt() ?: 0 }, Good)
                Text("daily load ${g.dailyLoad}", fontSize = 11.sp, color = Color.Gray)
                Spacer(Modifier.height(10.dp))
                Text("Intervals", style = MaterialTheme.typography.titleSmall)
                val maxInterval = g.intervals.maxOfOrNull { it.count.toInt() } ?: 1
                g.intervals.take(6).forEach { bucket ->
                    HBar("${bucket.days}d", bucket.count.toInt(), maxInterval, Good)
                }
                Text("Ease", style = MaterialTheme.typography.titleSmall)
                val maxEase = g.eases.maxOfOrNull { it.count.toInt() } ?: 1
                g.eases.take(5).forEach { bucket ->
                    HBar("${bucket.ease}%", bucket.count.toInt(), maxEase, Accent)
                }
                Text(String.format(java.util.Locale.US,
                    "average %.0f%% · retrievability %.1f%% · FSRS %s",
                    g.averageEase, g.retrievability * 100.0,
                    if (g.fsrs) "on" else "off"),
                    fontSize = 11.sp, color = Color.Gray)
                Spacer(Modifier.height(10.dp))
                Text("Hourly (all time)", style = MaterialTheme.typography.titleSmall)
                Spark(g.hours.map { it.total.toInt() }, Accent)
                Text("Answer buttons", style = MaterialTheme.typography.titleSmall)
                val maxButton = g.buttons.maxOfOrNull {
                    (it.learning + it.young + it.mature).toInt()
                } ?: 1
                g.buttons.forEach { row ->
                    val label = when (row.button.toInt()) {
                        1 -> "again"; 2 -> "hard"; 3 -> "good"; else -> "easy"
                    }
                    val total = (row.learning + row.young + row.mature).toInt()
                    HBar(label, total, maxButton, Again)
                    Text("  ${row.learning} learn / ${row.young} young / ${row.mature} mature",
                        fontSize = 10.sp, color = Color.Gray)
                }
                val retention = g.trueRetention
                val youngTotal = retention.youngPassed + retention.youngFailed
                val matureTotal = retention.maturePassed + retention.matureFailed
                fun pct(part: UInt, total: UInt) = if (total == 0u) "—"
                    else String.format(java.util.Locale.US, "%.0f%%",
                        part.toDouble() / total * 100.0)
                Text("Retention: young ${pct(retention.youngPassed, youngTotal)} " +
                    "($youngTotal) · mature ${pct(retention.maturePassed, matureTotal)} " +
                    "($matureTotal)", fontSize = 12.sp)
                Spacer(Modifier.height(16.dp))
                Text("Slowest cards", style = MaterialTheme.typography.titleMedium)
            }
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
