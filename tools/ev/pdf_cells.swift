// Extract the PBEV table as JSON Lines, one object per vehicle row.
//
// macOS-only helper for tools/ev/pbev_import.py. PDFKit can select the text
// inside a rectangle, so we anchor on the "Categoria" column to find each row
// and then select one cell per known column center.
//
// Usage: swift tools/ev/pdf_cells.swift <file.pdf> > rows.jsonl

import Foundation
import PDFKit

let args = CommandLine.arguments
guard args.count > 1, let doc = PDFDocument(url: URL(fileURLWithPath: args[1])) else {
    FileHandle.standardError.write("usage: pdf_cells.swift <file.pdf>\n".data(using: .utf8)!)
    exit(2)
}

// Column centers measured on the 2026/18th-cycle table.
let centers: [Double] = [
    185.0, 218.7, 249.7, 283.0, 305.0, 319.7, 336.5, 351.3, 366.2, 382.8,
    396.4, 409.7, 420.4, 431.1, 440.3, 448.3, 457.7, 467.7, 476.7, 486.6,
    499.3, 511.1, 520.9, 533.4, 546.4, 557.7, 568.6, 579.7, 593.6, 610.6,
    629.0, 648.9, 667.5,
]

// Longest first: "Utilitário Esportivo Grande 4x4" must win over "Grande".
let categories: [String] = [
    "Utilitário Esportivo Compacto", "Utilitário Esportivo Médio",
    "Utilitário Esportivo Grande", "Fora de Estrada Grande", "Fora de Estrada",
    "Picape Compacta", "Coupé",
    "Sub Compacto", "Extra Grande", "Picape", "Comercial", "Minivan",
    "Esportivo", "Compacto", "Grande", "Médio",
].sorted { $0.count > $1.count }

func boundaries(_ index: Int) -> (Double, Double) {
    let left = index == 0 ? 175.0 : (centers[index - 1] + centers[index]) / 2.0
    let right = index == centers.count - 1 ? 690.0 : (centers[index] + centers[index + 1]) / 2.0
    return (left, right)
}

for pageIndex in 0..<doc.pageCount {
    guard let page = doc.page(at: pageIndex),
          let whole = page.selection(for: page.bounds(for: .mediaBox)) else { continue }

    for line in whole.selectionsByLine() {
        let box = line.bounds(for: page)
        guard box.origin.x < 200, let raw = line.string else { continue }
        let text = raw.trimmingCharacters(in: .whitespacesAndNewlines)
        // Some rows merge the category cell with its neighbour, so match prefixes.
        guard categories.contains(where: { text.hasPrefix($0) }) else { continue }

        let rowY = box.origin.y - 0.35
        let rowH = box.size.height + 0.7
        var cells: [String] = []
        cells.reserveCapacity(centers.count)
        for index in centers.indices {
            let (left, right) = boundaries(index)
            let rect = CGRect(x: left, y: rowY, width: right - left, height: rowH)
            let value = (page.selection(for: rect)?.string ?? "")
                .trimmingCharacters(in: .whitespacesAndNewlines)
            cells.append(value)
        }
        let object: [String: Any] = ["p": pageIndex, "y": box.origin.y, "cells": cells]
        guard let data = try? JSONSerialization.data(withJSONObject: object),
              let json = String(data: data, encoding: .utf8) else { continue }
        print(json)
    }
}
