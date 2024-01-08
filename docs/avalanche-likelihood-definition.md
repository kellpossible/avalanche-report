---
bibliography: [Forecasting.bib]
---

# Avalanche Likelihood Definition

TODO: Include the points in https://github.com/kellpossible/avalanche-report/issues/52 and reference this paper.

*A Conceptual Model for Avalanche Hazard (CMAH)* @stathamConceptualModelAvalanche2018 is one of the most cited papers used as the basis for modern avalanche forecasting (CITATION NEEDED). One of the terms that it attempts to define is “Likelihood of avalanches”:

> *Likelihood of avalanche(s) is the chance of an avalanche releasing within a specific location and time period, regardless of avalanche size. While probability is dependent on scale, in practice forecasters express their likelihood judgments independently of scale, using qualitative terms such as possible or almost certain ( @stathamAVALANCHEHAZARDDANGER2008 ) across different scales.* 
>
> @stathamConceptualModelAvalanche2018

The team working on producing this forecast has a background in the New Zealand and Canadian avalanche forecasting systems. It was argued by experience that "Likelihood" can be a useful tool for communicating information about the seriousness of a particular avalanche problem.

The definition as provided by CMAH has several shortcomings which can cause confusion even among avalanche forecasters:

* It does not define what "specific location" is. Is it any possible location within the forecast area? Or any possible location that is avalanche terrain? Or is it any possible location within the elevation bands and aspects that the problem type is has been found in? There are many possible interpretations.
* What is the mechanism of release? If it is human triggered, what kind of load is being applied?
* It attempts to redefine the term "Likelihood" in which is already synomous with "Probability" in the English Language. This is especially problematic from a standpoint of people consuming the forecast for whom English may be their second language or they may be relying on automatic translation.
* It states that likelihood is independent of scale, and yet by choosing to present likelihood as a graphic in forecasts it has every appearance of possessing a scale as it is most commonly used. The appearance of the graphic impacts how the scale is interpreted. TODO insert figures of graphics.
* In practice, forecasters map the scale that they calculate in their heads (or with a matrix derived from other scales) to the value terms that are provided ("Possible", "Unlikely", etc), so in effect it can be seen to represent a scale and "Likelihood" by the CMAH definition cannot entirely escape it.
* The use of language terms for likelihood values could introduce extra ambiguity in translation to other languages, and for those who are reading in their second language. This issue is partially highlighted in ADAM @mullerCOMBININGCONCEPTUALMODEL2016 *"We find the terms used in the EDS and the BM ambiguous and partially redundant. The different languages in Europe pose certain challenges while identifying common terms. A literal translation of one term into another language sometimes is accompanied by a slight change in meaning or common perception of this term. This potential change in perception of one term could lead to a
different perception in the avalanche danger assessment. A translation should aim at adhering to the definition rather than being literally correct. This will require a careful translation process"*

Some of these points and more are discussed in *The Likelihood Scale in Avalanche Forecasting* @LikelihoodScaleAvalanche

There is a struggle here between wanting to provide a value convenient for communication, and poviding a framework which leaves little room for disagreement between those attempting to produce the forecast.


With this in mind we still sought to use this property in our forecasting software as a tool for communication, and to find a way to solve these shortcomings. Seeking a more rigorous definition for "Likelihood" we took the scale and matrix defined by the ADAM paper  ( @mullerCOMBININGCONCEPTUALMODEL2016 ) which provides what is arguablya more specific definition of "Likelihood": Likelihood of triggering an avalanche of a specific avalanche problem, as a product of sensitivity to triggers (snowpack stability) and spatial distribution.

Because the definition of "Likelihood" according to ADAM is already purely derived from sensitivity and distribution, we implemented this value as automatically calculated via the matrix this paper provided in Figure 4. @mullerCOMBININGCONCEPTUALMODEL2016 , to reduce subjectivity of interpretation.

This caused some disagreement among the team while we discussed this, and the following downsides to employing "Likelihood" as defined ADAM were proposed:

* In the Canadian and New Zealand forecasting systems there are 5 values for likelihood instead of 4 as presented in the ADAM paper.
  * People who are used to those systems may find the difference confusing.
  * The extra resolution might be useful.
  * Having a definition based purely on a matrix might not fit how users interpret the value in practice.
* The likelihood of a naturally triggered avalanche occurring does not fit nicely within the mapping provided in Table 2 of @mullerCOMBININGCONCEPTUALMODEL2016 , for example some natural avalanches such as wet slabs and glide slabs may be difficult for human rider to trigger, and could be defined as "Unreactive", and yet their actual probability of triggering in a natural event may be higher.

Some arguments were raised against attempting to provide a precise definition for "Likelihood":

* Avalanche forecasting is already a process that involves large errors and many grey areas due to the lack of resources with observations, weather forecasting, and our limited understanding of avalanche and snow mechanics, and having precise definitions does little to help the situation. 
* Avalanche forecasting and communication is at least partially an "art" and having imprecise definitions leaves room for performing it.

As a counterpoint to these, the same arguments could be made against making precise observations in the field, and yet the industry has a focus on repeatability and accuracy as much as is practical given the limitations of available resources. Why should the logic be any different for the definitions of the terms we use in our forecast and which form its very basis? In a similar vein of logic, why make the situation any worse by perpetuating imprecision and sources of confusion in our forecasting process and presentation when the solutions (improved definitions) can easily be made available?

If "Likelihood" is just to be used only as a conceptual tool designed to simplify communication about avalanche problems we still need a way to agree on what value to assign to it given a particular situation, and how it should be presented in order to effectively communicate its meaning. This requires a definition, and the better the definition, the less likely there will be any disagreement, and the more useful it is as a tool for communication (CITATION NEEDED).

TODO: It would be good to have more clarification on is the "art" of forecasting as it has been referred to.

Improving the definition's precision does not necessarily rule out the "art" of forecasting or remove the control a forecaster can exert over the output of the forecast, using their experience to recognise patterns and thereby produce a forecast in a way that is not yet convenient or possible to be reliably codified and automated.
