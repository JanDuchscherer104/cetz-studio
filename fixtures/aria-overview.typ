// Isolated style study: existing A1 computation, no thesis replacement.
#import "/typst/shared/architecture.typ": *
#import "/typst/shared/symbols.typ": symb
#import "/typst/shared/math.typ": T
#set page(width: auto, height: auto, margin: 4mm, fill: white)
#set text(font: "New Computer Modern", size: 9pt, fill: rgb("17202A"))
#let n=content-at.with(y-scale:.65)
#align(center)[*Overview · A + B + C*]
#diagram(
 edge-stroke: .8pt + rgb("374151"), edge-corner-radius: 4pt, mark-scale: 70%, spacing: 3pt,
 n(18,0,<physical>,[*Physical inputs*\ #T(symb.frame.root,symb.frame.cq), #T(symb.frame.ct,symb.frame.cq)\ root scene moments],fill:input,width:auto,shape:shapes.parallelogram),
 n(71,0,<target>,[*Target relation*\ #T(symb.frame.cq,symb.frame.e)],fill:input,shape:shapes.parallelogram),
 n(124,0,<state>,[*Shared state*\ scene · $e$ · $#symb.rl.selected_pose_prefix$\ $#symb.rl.budget$ · $#symb.rl.requested_horizon$],fill:input,width:auto,shape:shapes.parallelogram),
 n(18,22,<trunk>,[*Physical trunk*\ linear · GELU · norm],width:auto),
 n(71,22,<projection>,[*Value embedding*],width:auto),
 n(124,22,<tokens>,[*State encoders*],width:auto),
 n(18,46,<feas>,[*Feasibility head*]),
 n(18,66,<logit>,[*Feasibility*\ $#symb.rl.feasibility_logits$],fill:output,width:auto,shape:shapes.pill),
 n(71,49,<attention>,[*Cross-attention*],width:auto),
 n(71,78,<concat>,[*Concatenate*\ $[bold(u)_i, bold(c)_i, bold(u)_i dot.o bold(c)_i]$],fill:join,width:auto,shape:shapes.hexagon),
 n(71,102,<decoder>,[*Scalar decoder*\ Linear · GELU\ Dropout · Linear],width:auto),
 n(124,102,<value>,[*Conditional value*\ $#symb.rl.conditional_q$],fill:output,width:auto,shape:shapes.pill),
 edge(<physical>,<trunk>,"-|>"),edge(<target>,<projection>,"-|>"),edge(<trunk>,<projection>,"-|>"),
 edge(<trunk>,<feas>,"-|>"),edge(<feas>,<logit>,"-|>"),
 edge(<state>,<tokens>,"-|>"),
 edge(<projection>,<attention>,"-|>",[$Q=#symb.rl.value_queries$\ #text(size:7pt)[$#symb.shape.Nq times d$]]),
 edge(<tokens>,(124mm,-31.85mm),<attention>,"-|>",[$K=V=#symb.rl.state_tokens$\ $5 times d$],label-pos:.70),
 edge((71mm,-22.75mm),(40mm,-22.75mm),(40mm,-50.7mm),<concat>,"-|>",[$bold(u)_i$],label-pos:.58),
 edge(<attention>,<concat>,"-|>",[$#symb.rl.candidate_context$\ #text(size:7pt)[$#symb.shape.Nq times d$]]),
 edge(<concat>,<decoder>,"-|>",[$#symb.shape.Nq times 3d$]),
 edge(<decoder>,<value>,"-|>",[$#symb.shape.Nq times 1$]),
)
#v(3pt)
#align(center)[#text(size:8pt)[Same weights for all candidate rows · action masking outside the scorer]]
